use super::*;
use std::sync::mpsc;
use windowdeck_protocol::queue;
use windowdeck_protocol::video::{AccessUnit, Assembler};

enum Event {
    Packet(u64, AccessUnit),
    Connected(u64, TcpStream),
    Lost,
    Stopped,
    Failed(String),
}

pub(super) fn run(target: Target, fullscreen: bool) -> Result<(), Box<dyn Error>> {
    // Initialize SDL, the renderer and decoder before acknowledging any host probe.
    // A local startup failure must not cause a virtual display to appear on the PC.
    let mut player = windowdeck_media::Player::new(fullscreen)?;
    let (stream, id, _, _, codec) = connect(&target, VideoCodec::H264Frames)?;
    if codec == VideoCodec::H264 {
        drop(player);
        emit(
            Level::Info,
            "native_player_fallback",
            &[("reason", "legacy_mpegts_host")],
        );
        return play_legacy(stream, id, &target, fullscreen, false);
    }
    let shutdown = Arc::new(Mutex::new(stream.try_clone()?));
    // Keep recovery tied to the session whose queued packets are being decoded.
    // The shared shutdown socket may already belong to a newer handshake.
    let mut session_socket = stream.try_clone()?;
    let mut receiving_session = id;
    let mut discarded_session = None;
    let stopping = Arc::new(AtomicBool::new(false));
    let (tx, rx) = queue::channel();
    let worker_stopping = Arc::clone(&stopping);
    let worker_shutdown = Arc::clone(&shutdown);
    let worker = thread::spawn(move || {
        let result = receive(stream, id, &target, &tx, &worker_stopping, &worker_shutdown);
        if let Err(e) = result {
            let _ =
                queue::send_with_timeout(&tx, Event::Failed(e.to_string()), queue::STALL_TIMEOUT);
        }
    });
    let mut session = None;
    let mut queue_times = windowdeck_diagnostics::Timings::default();
    let mut report = Instant::now();
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            if !player.poll()? {
                return Ok(());
            }
            match rx.try_recv() {
                Ok(item) => {
                    queue_times.record(item.produced.elapsed());
                    // Preserve encoded dependencies through brief stalls. Presentation
                    // can drop decoded frames without reconnecting the display.
                    // Control events (especially Stop) must never expire in this queue.
                    if let Event::Packet(id, _) = &item.value {
                        if discarded_session == Some(*id) {
                            continue;
                        }
                        if item.produced.elapsed() > queue::STALL_TIMEOUT {
                            emit(
                                Level::Warn,
                                "native_queue_expired",
                                &[
                                    ("session_id", &id.to_string()),
                                    ("age_ms", &item.produced.elapsed().as_millis().to_string()),
                                ],
                            );
                            if receiving_session == *id {
                                let _ = session_socket.shutdown(Shutdown::Both);
                            }
                            discarded_session = Some(*id);
                            player.reset()?;
                            session = None;
                            continue;
                        }
                    }
                    match item.value {
                        Event::Packet(id, unit) => {
                            if session != Some(id) {
                                if !unit.keyframe || unit.number != 0 {
                                    continue;
                                }
                                player.reset()?;
                                session = Some(id);
                            }
                            if let Err(e) = player.packet(&unit.payload, unit.number) {
                                emit(
                                    Level::Warn,
                                    "native_decode_reset",
                                    &[("error", &e.to_string())],
                                );
                                if receiving_session == id {
                                    let _ = session_socket.shutdown(Shutdown::Both);
                                }
                                discarded_session = Some(id);
                                player.reset()?;
                                session = None;
                            }
                        }
                        Event::Connected(id, socket) => {
                            receiving_session = id;
                            session_socket = socket;
                            discarded_session = None;
                        }
                        Event::Lost => {
                            player.reset()?;
                            session = None;
                        }
                        Event::Stopped => return Ok(()),
                        Event::Failed(e) => return Err(e.into()),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => thread::sleep(Duration::from_millis(1)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("el receptor integrado se cerró sin estado final".into());
                }
            }
            if report.elapsed() >= Duration::from_secs(1) {
                queue_times.report("native_receive_queue_metrics");
                report = Instant::now();
            }
        }
    })();
    stopping.store(true, Ordering::Relaxed);
    if let Ok(socket) = shutdown.lock() {
        let _ = socket.shutdown(Shutdown::Both);
    }
    drop(rx);
    worker
        .join()
        .map_err(|_| "el receptor integrado terminó inesperadamente")?;
    result
}

fn receive(
    mut stream: TcpStream,
    mut session: u64,
    target: &Target,
    output: &mpsc::SyncSender<queue::Queued<Event>>,
    stopping: &AtomicBool,
    shutdown: &Mutex<TcpStream>,
) -> io::Result<()> {
    let mut retry = connection::RetryBackoff::default();
    loop {
        let started = Instant::now();
        let mut assembler = Assembler::new(session);
        let mut receive_times = windowdeck_diagnostics::Timings::default();
        let mut report = Instant::now();
        let result = (|| -> Result<(), Box<dyn Error>> {
            loop {
                if stopping.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let started = Instant::now();
                let message = read_message(&mut stream)?;
                receive_times.record(started.elapsed());
                if message == Message::Stop {
                    if assembler.is_partial() {
                        return Err("Stop interrumpe un fotograma H.264".into());
                    }
                    queue::send_with_timeout(output, Event::Stopped, queue::STALL_TIMEOUT)?;
                    return Ok(());
                }
                if let Some(unit) = assembler.push(message)? {
                    queue::send_with_timeout(
                        output,
                        Event::Packet(session, unit),
                        queue::STALL_TIMEOUT,
                    )?;
                }
                if report.elapsed() >= Duration::from_secs(1) {
                    receive_times.report("native_receive_metrics");
                    report = Instant::now();
                }
            }
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(_) if stopping.load(Ordering::Relaxed) => return Ok(()),
            Err(e) if transport_error(e.as_ref()) => emit(
                Level::Warn,
                "native_connection_lost",
                &[("error", &e.to_string())],
            ),
            Err(e) => return Err(io::Error::other(e.to_string())),
        }
        let _ = stream.shutdown(Shutdown::Both);
        retry.session_ended(started.elapsed());
        queue::send_with_timeout(output, Event::Lost, queue::STALL_TIMEOUT)?;
        loop {
            let delay = retry.next_delay();
            emit(
                Level::Info,
                "connection_retry_wait",
                &[("seconds", &delay.as_secs().to_string())],
            );
            let until = Instant::now() + delay;
            while Instant::now() < until {
                if stopping.load(Ordering::Relaxed) {
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(20));
            }
            match connect_observed(target, VideoCodec::H264Frames, Some((stopping, shutdown))) {
                Ok((next, id, _, _, VideoCodec::H264Frames)) => {
                    let mut watched = shutdown
                        .lock()
                        .map_err(|_| io::Error::other("socket lock poisoned"))?;
                    if stopping.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    *watched = next.try_clone()?;
                    drop(watched);
                    stream = next;
                    session = id;
                    queue::send_with_timeout(
                        output,
                        Event::Connected(id, stream.try_clone()?),
                        queue::STALL_TIMEOUT,
                    )?;
                    emit(
                        Level::Info,
                        "native_reconnected",
                        &[("session_id", &id.to_string())],
                    );
                    break;
                }
                Ok(_) => {
                    return Err(io::Error::other(
                        "el host cambió de códec; vuelve a abrir el cliente",
                    ));
                }
                Err(e) if transport_error(e.as_ref()) => {}
                Err(e) => return Err(io::Error::other(e.to_string())),
            }
        }
    }
}
