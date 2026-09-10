use super::*;
use std::sync::mpsc;
use windowdeck_protocol::queue;
use windowdeck_protocol::video::{AccessUnit, Assembler};

enum Event {
    Packet(u64, AccessUnit),
    Lost,
    Stopped,
    Failed(String),
}

pub(super) fn run(target: Target, fullscreen: bool) -> Result<(), Box<dyn Error>> {
    let (stream, id, _, _, codec) = connect(&target, VideoCodec::H264Frames)?;
    if codec == VideoCodec::H264 {
        emit(
            Level::Info,
            "native_player_fallback",
            &[("reason", "legacy_mpegts_host")],
        );
        return play_legacy(stream, id, &target, fullscreen, false);
    }
    let mut player = windowdeck_media::Player::new(fullscreen)?;
    let shutdown = Arc::new(Mutex::new(stream.try_clone()?));
    let stopping = Arc::new(AtomicBool::new(false));
    let (tx, rx) = queue::channel();
    let worker_stopping = Arc::clone(&stopping);
    let worker_shutdown = Arc::clone(&shutdown);
    let worker = thread::spawn(move || {
        let result = receive(stream, id, &target, &tx, &worker_stopping, &worker_shutdown);
        if let Err(e) = result {
            let _ = queue::send(&tx, Event::Failed(e.to_string()));
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
                    // Expiration abandons the session; the next connection begins
                    // with an IDR. Never skip dependent fragments within a session.
                    let event = match item.into_fresh() {
                        Ok(event) => event,
                        Err(_) => {
                            if let Ok(socket) = shutdown.lock() {
                                let _ = socket.shutdown(Shutdown::Both);
                            }
                            player.reset()?;
                            session = None;
                            while rx.try_recv().is_ok() {}
                            continue;
                        }
                    };
                    match event {
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
                                if let Ok(socket) = shutdown.lock() {
                                    let _ = socket.shutdown(Shutdown::Both);
                                }
                                player.reset()?;
                                session = None;
                            }
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
    loop {
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
                    queue::send(output, Event::Stopped)?;
                    return Ok(());
                }
                if let Some(unit) = assembler.push(message)? {
                    queue::send(output, Event::Packet(session, unit))?;
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
        queue::send(output, Event::Lost)?;
        loop {
            let until = Instant::now() + RECONNECT_DELAY;
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
                    stream = next;
                    session = id;
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
