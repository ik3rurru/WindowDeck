use crate::{
    model::{Panel, State},
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
    start: nwg::Button,
    stop: nwg::Button,
    logs: nwg::Button,
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
        self.status.set_text(model.text());
        self.start
            .set_enabled(model.state == State::Stopped && !self.closing.get());
        self.stop
            .set_enabled(model.state != State::Stopped && model.state != State::Stopping);
        self.logs.set_enabled(self.session.borrow().is_some());
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
            Event::Finished(result) => {
                self.model.borrow_mut().finish(result);
                return true;
            }
        }
        false
    }
}

pub fn run(native: bool, smoke: Option<PathBuf>) -> io::Result<()> {
    let started = Instant::now();
    let instance = single_instance::SingleInstance::new("Local\\WindowDeck.Launcher")
        .map_err(|e| platform::error(e.to_string()))?;
    if !instance.is_single() {
        return Err(platform::error("WindowDeck ya está abierto."));
    }
    nwg::init().map_err(nwg_error)?;
    nwg::Font::set_global_family("Segoe UI").map_err(nwg_error)?;
    let mut ui = Ui::default();
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
        .size((520, 240))
        .center(true)
        .title(if native {
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
        .position((20, 20))
        .size((480, 65))
        .parent(&ui.window)
        .build(&mut ui.status)
        .map_err(nwg_error)?;
    for (button, text, x, width) in [
        (&mut ui.start, "&Iniciar", 20, 145),
        (&mut ui.stop, "&Detener", 185, 145),
        (&mut ui.logs, "Ver &registros", 350, 150),
    ] {
        nwg::Button::builder()
            .text(text)
            .position((x, 100))
            .size((width, 40))
            .parent(&ui.window)
            .build(button)
            .map_err(nwg_error)?;
    }
    nwg::Label::builder()
        .text("Buscando la dirección del PC...")
        .position((20, 165))
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
    ui.refresh();
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
            nwg::Event::OnButtonClick if handle == ui.start => {
                if ui.worker.borrow().is_none() && !ui.closing.get() {
                    ui.model.borrow_mut().start();
                    *ui.worker.borrow_mut() = Some(platform::start(native));
                    ui.refresh();
                }
            }
            nwg::Event::OnButtonClick if handle == ui.stop => ui.stop(),
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
