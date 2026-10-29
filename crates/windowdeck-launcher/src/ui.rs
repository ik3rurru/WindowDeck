use crate::{
    cli::SessionMode,
    model::{self, Panel, State},
    platform::{self, Event, Worker},
};
use native_windows_gui as nwg;
use std::{
    cell::{Cell, RefCell},
    fs, io,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Ui {
    window: nwg::Window,
    status: nwg::Label,
    extend: nwg::RadioButton,
    mirror: nwg::RadioButton,
    mode_hint: nwg::Label,
    start: nwg::Button,
    stop: nwg::Button,
    logs: nwg::Button,
    accept_pairing: nwg::Button,
    reject_pairing: nwg::Button,
    devices_label: nwg::Label,
    devices: nwg::ComboBox<String>,
    revoke: nwg::Button,
    paired: RefCell<Vec<windowdeck_transport::Device>>,
    address: nwg::Label,
    timer: nwg::AnimationTimer,
    icon: nwg::Icon,
    model: RefCell<Panel>,
    worker: RefCell<Option<Worker>>,
    session: RefCell<Option<PathBuf>>,
    closing: Cell<bool>,
}

impl Ui {
    fn refresh(&self) {
        let model = self.model.borrow();
        self.status.set_text(&model.text());
        let pending = model.pairing.as_ref();
        for button in [&self.accept_pairing, &self.reject_pairing] {
            button.set_visible(pending.is_some());
            button.set_enabled(pending.is_some_and(|request| !request.answered));
        }
        // Revocation applies to the next connection; offer it between sessions.
        self.revoke.set_enabled(
            model.state == State::Stopped
                && !self.closing.get()
                && self.devices.selection().is_some(),
        );
        self.start
            .set_enabled(model.state == State::Stopped && !self.closing.get());
        self.stop
            .set_enabled(model.state != State::Stopped && model.state != State::Stopping);
        self.logs.set_enabled(self.session.borrow().is_some());
        let editable = model.state == State::Stopped && !self.closing.get();
        self.extend.set_enabled(editable);
        self.mirror.set_enabled(editable);
        self.mode_hint
            .set_text(if model.mode == SessionMode::Mirror {
                "Muestra la pantalla principal del PC en la Deck. No necesita el driver virtual."
            } else {
                "Añade una pantalla para mover ventanas a la Deck. Requiere el driver virtual."
            });
    }

    fn stop(&self) {
        if let Some(worker) = self.worker.borrow().as_ref() {
            worker.stop();
            self.model.borrow_mut().stop();
        }
        self.refresh();
    }

    fn tick(&self) {
        let mut finished = false;
        if let Some(worker) = self.worker.borrow().as_ref() {
            for event in worker.events.try_iter() {
                finished |= self.apply_event(event);
            }
            if !finished && worker.finished() {
                // The thread may have finished immediately after draining. Read
                // once more before treating a missing completion as a panic.
                for event in worker.events.try_iter() {
                    finished |= self.apply_event(event);
                }
                if !finished {
                    self.model.borrow_mut().finish(Err(
                        "La sesión terminó inesperadamente. Consulta los registros.".into(),
                    ));
                }
                finished = true;
            }
        }
        if finished {
            self.worker.borrow_mut().take();
        }
        self.refresh();
        if self.closing.get() && self.worker.borrow().is_none() {
            nwg::stop_thread_dispatch();
        }
    }

    fn apply_event(&self, event: Event) -> bool {
        match event {
            Event::Session(path) => *self.session.borrow_mut() = Some(path),
            Event::State(state) => self.model.borrow_mut().update(state),
            Event::Pairing(line) => {
                let request = matches!(line, model::HostLine::Pairing(_));
                if self.model.borrow_mut().pairing_event(line) {
                    self.reload_devices();
                }
                if request {
                    // Bring the panel forward: the request expires in two minutes.
                    self.window.set_visible(true);
                    self.window.set_focus();
                }
            }
            Event::Finished(result) => {
                self.model.borrow_mut().finish(result);
                self.reload_devices();
                return true;
            }
        }
        false
    }

    fn answer_pairing(&self, accept: bool) {
        let Some(request) = self.model.borrow_mut().answer() else {
            return;
        };
        let written = match self.session.borrow().as_ref() {
            Some(session) => model::answer_pairing(session, &request.id, accept),
            None => Err(io::Error::other("no hay sesión activa")),
        };
        if let Err(e) = written {
            nwg::modal_error_message(
                &self.window,
                "WindowDeck",
                &format!("No se pudo enviar la respuesta al host: {e}"),
            );
        }
        self.refresh();
    }

    fn reload_devices(&self) {
        let devices = windowdeck_transport::Trust::open_default(windowdeck_transport::Role::Host)
            .and_then(|trust| trust.devices());
        let (label, items, devices) = match devices {
            Ok(devices) if devices.is_empty() => (
                "Dispositivos emparejados: ninguno. La primera conexión pedirá un código.".into(),
                Vec::new(),
                devices,
            ),
            Ok(devices) => (
                format!("Dispositivos emparejados ({}):", devices.len()),
                devices
                    .iter()
                    .map(|device| {
                        format!(
                            "{} · {} · {}",
                            device.name,
                            device.key.fingerprint(),
                            device.paired_on()
                        )
                    })
                    .collect(),
                devices,
            ),
            Err(e) => (
                format!("No se pudo leer la lista de dispositivos: {e}"),
                Vec::new(),
                Vec::new(),
            ),
        };
        self.devices_label.set_text(&label);
        let selected = (!items.is_empty()).then_some(0);
        self.devices.set_collection(items);
        self.devices.set_selection(selected);
        *self.paired.borrow_mut() = devices;
        self.refresh();
    }

    fn revoke_selected(&self) {
        let Some(device) = self
            .devices
            .selection()
            .and_then(|index| self.paired.borrow().get(index).cloned())
        else {
            return;
        };
        let choice = nwg::modal_message(
            &self.window,
            &nwg::MessageParams {
                title: "WindowDeck",
                content: &format!(
                    "¿Revocar «{}» (huella {})? No podrá conectarse hasta emparejarse de nuevo.",
                    device.name,
                    device.key.fingerprint()
                ),
                buttons: nwg::MessageButtons::YesNo,
                icons: nwg::MessageIcons::Warning,
            },
        );
        if choice != nwg::MessageChoice::Yes {
            return;
        }
        let removed = windowdeck_transport::Trust::open_default(windowdeck_transport::Role::Host)
            .and_then(|trust| trust.remove_key(&device.key));
        if let Err(e) = removed {
            nwg::modal_error_message(
                &self.window,
                "WindowDeck",
                &format!("No se pudo revocar el dispositivo: {e}"),
            );
        }
        self.reload_devices();
    }
}

pub fn run(mode: SessionMode, smoke: Option<PathBuf>) -> io::Result<()> {
    let started = Instant::now();
    let instance = single_instance::SingleInstance::new("Local\\WindowDeck.Launcher")
        .map_err(|e| platform::error(e.to_string()))?;
    if !instance.is_single() {
        return Err(platform::error("WindowDeck ya está abierto."));
    }
    nwg::init().map_err(nwg_error)?;
    nwg::Font::set_global_family("Segoe UI").map_err(nwg_error)?;
    let mut ui = Ui::default();
    ui.model.get_mut().mode = mode;
    let mut resources = nwg::EmbedResource::default();
    nwg::EmbedResource::builder()
        .build(&mut resources)
        .map_err(nwg_error)?;
    nwg::Icon::builder()
        .source_embed(Some(&resources))
        .source_embed_id(1)
        .build(&mut ui.icon)
        .map_err(nwg_error)?;
    nwg::Window::builder()
        .size((520, 470))
        .center(true)
        .title(if mode == SessionMode::Native {
            "WindowDeck — GPU experimental"
        } else {
            "WindowDeck"
        })
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .icon(Some(&ui.icon))
        .build(&mut ui.window)
        .map_err(nwg_error)?;
    nwg::Label::builder()
        .text(State::Stopped.text())
        .position((20, 135))
        .size((480, 65))
        .parent(&ui.window)
        .build(&mut ui.status)
        .map_err(nwg_error)?;
    for (radio, text, y, checked, group) in [
        (
            &mut ui.extend,
            "&Extender escritorio",
            16,
            mode != SessionMode::Mirror,
            true,
        ),
        (
            &mut ui.mirror,
            "&Duplicar pantalla principal",
            47,
            mode == SessionMode::Mirror,
            false,
        ),
    ] {
        let mut flags = nwg::RadioButtonFlags::VISIBLE | nwg::RadioButtonFlags::TAB_STOP;
        if group {
            flags |= nwg::RadioButtonFlags::GROUP;
        }
        nwg::RadioButton::builder()
            .text(text)
            .flags(flags)
            .position((20, y))
            .size((480, 28))
            .check_state(if checked {
                nwg::RadioButtonState::Checked
            } else {
                nwg::RadioButtonState::Unchecked
            })
            .parent(&ui.window)
            .build(radio)
            .map_err(nwg_error)?;
    }
    nwg::Label::builder()
        .position((20, 84))
        .size((480, 45))
        .parent(&ui.window)
        .build(&mut ui.mode_hint)
        .map_err(nwg_error)?;
    for (button, text, x, width) in [
        (&mut ui.start, "&Iniciar", 20, 145),
        (&mut ui.stop, "&Detener", 185, 145),
        (&mut ui.logs, "Ver &registros", 350, 150),
    ] {
        nwg::Button::builder()
            .text(text)
            .position((x, 215))
            .size((width, 40))
            .parent(&ui.window)
            .build(button)
            .map_err(nwg_error)?;
    }
    for (button, text, x, width) in [
        (&mut ui.accept_pairing, "&Aceptar emparejamiento", 20, 230),
        (&mut ui.reject_pairing, "Rec&hazar", 260, 150),
    ] {
        nwg::Button::builder()
            .text(text)
            .position((x, 265))
            .size((width, 36))
            .flags(nwg::ButtonFlags::TAB_STOP)
            .parent(&ui.window)
            .build(button)
            .map_err(nwg_error)?;
    }
    nwg::Label::builder()
        .position((20, 315))
        .size((480, 22))
        .parent(&ui.window)
        .build(&mut ui.devices_label)
        .map_err(nwg_error)?;
    nwg::ComboBox::builder()
        .position((20, 340))
        .size((330, 32))
        .parent(&ui.window)
        .build(&mut ui.devices)
        .map_err(nwg_error)?;
    nwg::Button::builder()
        .text("Re&vocar")
        .position((360, 338))
        .size((140, 32))
        .parent(&ui.window)
        .build(&mut ui.revoke)
        .map_err(nwg_error)?;
    nwg::Label::builder()
        .text("Buscando la dirección del PC...")
        .position((20, 390))
        .size((480, 65))
        .parent(&ui.window)
        .build(&mut ui.address)
        .map_err(nwg_error)?;
    nwg::AnimationTimer::builder()
        .parent(&ui.window)
        .interval(Duration::from_millis(100))
        .active(true)
        .build(&mut ui.timer)
        .map_err(nwg_error)?;
    let (addresses_tx, addresses_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = addresses_tx.send(platform::addresses());
    });
    let ui = Rc::new(ui);
    ui.reload_devices();
    ui.start.set_focus();
    if let Some(path) = &smoke {
        fs::create_dir_all(path)?;
        fs::write(
            path.join("startup.txt"),
            format!(
                "ready_ms={}\nscale_factor={}\nlogical_size={:?}\n",
                started.elapsed().as_millis(),
                nwg::scale_factor(),
                ui.window.size()
            ),
        )?;
    }
    let weak = Rc::downgrade(&ui);
    let smoke_started = Instant::now();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |event, data, handle| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        match event {
            nwg::Event::OnButtonClick if handle == ui.extend || handle == ui.mirror => {
                if ui.model.borrow().state == State::Stopped && !ui.closing.get() {
                    ui.model.borrow_mut().mode = if handle == ui.mirror {
                        SessionMode::Mirror
                    } else if mode == SessionMode::Native {
                        SessionMode::Native
                    } else {
                        SessionMode::Extend
                    };
                    ui.refresh();
                }
            }
            nwg::Event::OnButtonClick if handle == ui.start => {
                if ui.worker.borrow().is_none() && !ui.closing.get() {
                    ui.model.borrow_mut().start();
                    *ui.worker.borrow_mut() = Some(platform::start(ui.model.borrow().mode));
                    ui.refresh();
                }
            }
            nwg::Event::OnButtonClick if handle == ui.stop => ui.stop(),
            nwg::Event::OnButtonClick if handle == ui.accept_pairing => ui.answer_pairing(true),
            nwg::Event::OnButtonClick if handle == ui.reject_pairing => ui.answer_pairing(false),
            nwg::Event::OnButtonClick if handle == ui.revoke => ui.revoke_selected(),
            nwg::Event::OnComboxBoxSelection if handle == ui.devices => ui.refresh(),
            nwg::Event::OnButtonClick if handle == ui.logs => {
                if let Some(path) = ui.session.borrow().as_ref()
                    && let Err(e) = platform::open_logs(path)
                {
                    nwg::modal_error_message(
                        &ui.window,
                        "WindowDeck",
                        &format!("No se pueden abrir los registros: {e}"),
                    );
                }
            }
            nwg::Event::OnWindowClose if handle == ui.window => {
                if let nwg::EventData::OnWindowClose(close) = data {
                    close.close(false);
                }
                ui.closing.set(true);
                ui.stop();
                ui.tick();
            }
            nwg::Event::OnTimerTick if handle == ui.timer => {
                if let Ok(addresses) = addresses_rx.try_recv() {
                    ui.address.set_text(&addresses);
                }
                ui.tick();
                if let Some(path) = &smoke {
                    if path.join("close.request").is_file() {
                        let _ = fs::write(path.join("closed.txt"), "normal_close=true\n");
                        ui.window.close();
                    } else if smoke_started.elapsed() >= Duration::from_secs(60) {
                        // Bound abandoned smoke runs, but do not report them as success.
                        let _ = fs::write(path.join("timeout.txt"), "inspection_timeout=true\n");
                        ui.window.close();
                    }
                }
            }
            _ => {}
        }
    });
    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);
    Ok(())
}

fn nwg_error(error: nwg::NwgError) -> io::Error {
    platform::error(error.to_string())
}
