#![cfg_attr(windows, windows_subsystem = "windows")]

mod cli;
#[cfg(any(windows, test))]
mod model;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mode = match cli::parse(std::env::args_os().skip(1)) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("{error}\n{}", cli::HELP);
            return ExitCode::from(2);
        }
    };
    match mode {
        cli::Mode::Help => println!("{}", cli::HELP),
        cli::Mode::Version => println!("WindowDeck Launcher {}", env!("CARGO_PKG_VERSION")),
        #[cfg(windows)]
        cli::Mode::Broker(options) => {
            if let Err(error) = platform::broker(options) {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        }
        #[cfg(windows)]
        cli::Mode::Panel { native, smoke } => {
            let interactive = smoke.is_none();
            if let Err(error) = ui::run(native, smoke) {
                if interactive {
                    native_windows_gui::error_message("WindowDeck", &error.to_string());
                } else {
                    eprintln!("{error}");
                }
                return ExitCode::FAILURE;
            }
        }
        #[cfg(not(windows))]
        _ => {
            eprintln!(
                "El panel de WindowDeck requiere Windows. En la Deck utiliza windowdeck-client."
            );
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
