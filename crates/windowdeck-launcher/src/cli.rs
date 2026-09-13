use std::{ffi::OsString, net::SocketAddr, path::PathBuf};

pub const HELP: &str = "WindowDeck Launcher
Uso: windowdeck-launcher [--native]
     windowdeck-launcher --help | --version
     windowdeck-launcher --ui-smoke-test DIRECTORIO

Sin opciones: panel de Windows, ruta CPU/libx264.
--native: ruta GPU experimental (requiere native-media).
--ui-smoke-test: abre y cierra el panel sin iniciar una sesión; guarda medidas locales.";

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Help,
    Version,
    Panel {
        native: bool,
        smoke: Option<PathBuf>,
    },
    Broker(BrokerOptions),
}

#[derive(Debug, PartialEq, Eq)]
pub struct BrokerOptions {
    pub address: SocketAddr,
    pub token: String,
    pub session: PathBuf,
    pub native: bool,
}

pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Mode, &'static str> {
    let args: Vec<_> = args.collect();
    let panel = |native, smoke| Ok(Mode::Panel { native, smoke });
    match args.as_slice() {
        [] => panel(false, None),
        [arg] if arg == "--help" || arg == "-h" => Ok(Mode::Help),
        [arg] if arg == "--version" || arg == "-V" => Ok(Mode::Version),
        [arg] if arg == "--native" => panel(true, None),
        [arg, path] if arg == "--ui-smoke-test" && !path.is_empty() => {
            panel(false, Some(path.into()))
        }
        [arg, address, token, session, route] if arg == "--broker" => {
            let address: SocketAddr = address
                .to_str()
                .and_then(|s| s.parse().ok())
                .filter(|a: &SocketAddr| a.ip() == std::net::Ipv4Addr::LOCALHOST && a.port() != 0)
                .ok_or("Dirección interna inválida")?;
            let token = token
                .to_str()
                .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or("Identificador interno inválido")?
                .to_owned();
            let session = PathBuf::from(session);
            if !session.is_absolute() {
                return Err("La carpeta de sesión debe ser absoluta");
            }
            let native = if route == "gpu" {
                true
            } else if route == "cpu" {
                false
            } else {
                return Err("Ruta interna inválida");
            };
            Ok(Mode::Broker(BrokerOptions {
                address,
                token,
                session,
                native,
            }))
        }
        _ => Err("Argumentos inválidos"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_str(args: &[&str]) -> Result<Mode, &'static str> {
        parse(args.iter().map(OsString::from))
    }
    #[test]
    fn normal_launch_uses_cpu_and_rejects_ambiguous_arguments() {
        assert_eq!(
            parse_str(&[]),
            Ok(Mode::Panel {
                native: false,
                smoke: None
            })
        );
        assert_eq!(
            parse_str(&["--native"]),
            Ok(Mode::Panel {
                native: true,
                smoke: None
            })
        );
        for args in [
            &["--native", "--native"][..],
            &["--version", "--native"],
            &["--broker"],
            &["-Native"],
            &["--ui-smoke-test", ""],
        ] {
            assert!(parse_str(args).is_err());
        }
    }
    #[test]
    fn broker_rejects_remote_endpoints_and_malformed_capabilities() {
        let path = std::env::temp_dir();
        let path = path.to_str().unwrap();
        let token = "0123456789abcdef0123456789abcdef";
        assert!(parse_str(&["--broker", "127.0.0.1:12345", token, path, "cpu"]).is_ok());
        for address in [
            "0.0.0.0:12345",
            "192.168.1.1:12345",
            "127.0.0.1:0",
            "localhost:12345",
        ] {
            assert!(parse_str(&["--broker", address, token, path, "cpu"]).is_err());
        }
        assert!(parse_str(&["--broker", "127.0.0.1:12345", "bad", path, "cpu"]).is_err());
        assert!(parse_str(&["--broker", "127.0.0.1:12345", token, "relative", "cpu"]).is_err());
    }
}
