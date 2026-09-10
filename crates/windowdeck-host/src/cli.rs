use super::{CaptureTarget, DEFAULT_ADDRESS, Mode};
use windowdeck_protocol::VideoCodec;

pub(super) const HELP: &str = "WindowDeck Host

Uso recomendado (Windows, con --frame-broker iniciado):
  windowdeck-host --driver-h264 [DIRECCION]

Sin argumentos se utiliza la misma ruta CPU H.264.
DIRECCION predeterminada: 0.0.0.0:48150

  --help, -h                    Mostrar esta ayuda
  --version, -V                 Mostrar versiones y funciones compiladas
  diag --help                   Pruebas y rutas historicas de desarrollo
  --driver-native-h264 [DIR]    Ruta GPU experimental; requiere native-media

El lanzador WindowDeck administra el broker y la sesion.
Emparejamiento y cifrado pendientes: usar solo una LAN de confianza.";

pub(super) const DIAG_HELP: &str = "Diagnosticos de WindowDeck (desarrollo)

  windowdeck-host diag capture [MONITOR]           Una textura D3D11
  windowdeck-host diag encode [MONITOR]            Codificar 60 frames en memoria
  windowdeck-host diag driver-frames              Validar 120 frames CPU del driver
  windowdeck-host diag gpu-frames                 Validar 120 texturas compartidas
  windowdeck-host diag gpu-encode [ENCODER]        Ensayo sintetico integrado

MONITOR comienza en 1 y vale 1 por defecto.
ENCODER: auto (predeterminado), libx264, h264_amf o h264_nvenc.
gpu-encode requiere Windows y native-media. Los ensayos del driver requieren
su broker correspondiente y una sesion de escritorio inactiva.

Referencias historicas congeladas (no recomendadas para uso normal):
  windowdeck-host diag pattern [DIRECCION]                  Patron RGB332
  windowdeck-host diag capture-stream MONITOR [DIRECCION]   Captura RGB332
  windowdeck-host diag h264-stream MONITOR [DIRECCION]      Captura ddagrab
  windowdeck-host diag virtual-h264 [DIRECCION]              WGC, monitor manual
  windowdeck-host diag auto-virtual-h264 [DIRECCION]         WGC, monitor automatico

DIRECCION predeterminada: 0.0.0.0:48150
Requisitos, migracion de flags y mediciones: docs/development.md.";

pub(super) fn parse_mode(args: impl IntoIterator<Item = String>) -> Result<Mode, &'static str> {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("--help" | "-h") => finish(args, Mode::Help),
        Some("--version" | "-V") => finish(args, Mode::Version),
        Some("diag") => parse_diag(args),
        None | Some("--driver-h264") => {
            serve(args, Some(CaptureTarget::WindowDeckCpu), VideoCodec::H264)
        }
        Some("--driver-native-h264") => serve(
            args,
            Some(CaptureTarget::WindowDeckNative),
            VideoCodec::H264Frames,
        ),
        Some(
            "--capture-test"
            | "--encode-test"
            | "--driver-frame-test"
            | "--gpu-frame-test"
            | "--gpu-self-test"
            | "--capture"
            | "--h264"
            | "--virtual-h264"
            | "--auto-virtual-h264",
        ) => Err(
            "las pruebas y rutas historicas se han movido a 'windowdeck-host diag'; consulta 'windowdeck-host diag --help'",
        ),
        _ => Err("usa windowdeck-host --driver-h264 [DIRECCION] o consulta --help"),
    }
}

fn parse_diag(mut args: impl Iterator<Item = String>) -> Result<Mode, &'static str> {
    match args.next().as_deref() {
        None | Some("--help" | "-h") => finish(args, Mode::DiagHelp),
        Some(command @ ("capture" | "encode")) => {
            let index = monitor(args.next().as_deref().unwrap_or("1"))?;
            finish(
                args,
                if command == "capture" {
                    Mode::CaptureTest(index)
                } else {
                    Mode::EncodeTest(index)
                },
            )
        }
        Some("driver-frames") => finish(args, Mode::DriverFrameTest),
        Some("gpu-frames") => finish(args, Mode::GpuFrameTest),
        Some("gpu-encode") => {
            let encoder = args.next().unwrap_or_else(|| "auto".into());
            if !matches!(
                encoder.as_str(),
                "auto" | "libx264" | "h264_amf" | "h264_nvenc"
            ) {
                return Err("encoder invalido; usa auto, libx264, h264_amf o h264_nvenc");
            }
            finish(args, Mode::GpuEncodeTest(encoder))
        }
        Some("pattern") => serve(args, None, VideoCodec::Rgb332),
        Some(command @ ("capture-stream" | "h264-stream")) => {
            let index = monitor(&args.next().ok_or("falta el indice del monitor")?)?;
            serve(
                args,
                Some(CaptureTarget::Monitor(index)),
                if command == "h264-stream" {
                    VideoCodec::H264
                } else {
                    VideoCodec::Rgb332
                },
            )
        }
        Some("virtual-h264") => serve(args, Some(CaptureTarget::WindowDeck), VideoCodec::H264),
        Some("auto-virtual-h264") => {
            serve(args, Some(CaptureTarget::WindowDeckAuto), VideoCodec::H264)
        }
        _ => Err("diagnostico desconocido; consulta 'windowdeck-host diag --help'"),
    }
}

fn monitor(value: &str) -> Result<usize, &'static str> {
    value
        .parse()
        .ok()
        .filter(|index| *index > 0)
        .ok_or("el indice del monitor debe ser un entero mayor que cero")
}

fn finish(mut args: impl Iterator<Item = String>, mode: Mode) -> Result<Mode, &'static str> {
    if args.next().is_some() {
        return Err("sobran argumentos; consulta --help o 'diag --help'");
    }
    Ok(mode)
}

fn serve(
    mut args: impl Iterator<Item = String>,
    monitor: Option<CaptureTarget>,
    codec: VideoCodec,
) -> Result<Mode, &'static str> {
    let address = args.next().unwrap_or_else(|| DEFAULT_ADDRESS.into());
    if address.is_empty() || address.starts_with('-') {
        return Err("se esperaba una direccion de escucha, no una opcion");
    }
    finish(
        args,
        Mode::Serve {
            address,
            monitor,
            codec,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Mode, &'static str> {
        parse_mode(args.iter().map(|arg| (*arg).into()))
    }

    #[test]
    fn normal_start_always_selects_the_supported_cpu_route() {
        for args in [&[][..], &["--driver-h264"][..]] {
            assert_eq!(
                parse(args),
                Ok(Mode::Serve {
                    address: DEFAULT_ADDRESS.into(),
                    monitor: Some(CaptureTarget::WindowDeckCpu),
                    codec: VideoCodec::H264,
                })
            );
        }
        assert_eq!(
            parse(&["--driver-h264", "127.0.0.1:48151"]),
            Ok(Mode::Serve {
                address: "127.0.0.1:48151".into(),
                monitor: Some(CaptureTarget::WindowDeckCpu),
                codec: VideoCodec::H264,
            })
        );
        assert_eq!(
            parse(&["--driver-native-h264"]),
            Ok(Mode::Serve {
                address: DEFAULT_ADDRESS.into(),
                monitor: Some(CaptureTarget::WindowDeckNative),
                codec: VideoCodec::H264Frames,
            })
        );
    }

    #[test]
    fn diagnostics_select_the_existing_probes() {
        for (args, expected) in [
            (vec!["diag", "capture"], Mode::CaptureTest(1)),
            (vec!["diag", "capture", "2"], Mode::CaptureTest(2)),
            (vec!["diag", "encode"], Mode::EncodeTest(1)),
            (vec!["diag", "encode", "2"], Mode::EncodeTest(2)),
            (vec!["diag", "driver-frames"], Mode::DriverFrameTest),
            (vec!["diag", "gpu-frames"], Mode::GpuFrameTest),
            (
                vec!["diag", "gpu-encode"],
                Mode::GpuEncodeTest("auto".into()),
            ),
            (
                vec!["diag", "gpu-encode", "libx264"],
                Mode::GpuEncodeTest("libx264".into()),
            ),
        ] {
            assert_eq!(parse(&args), Ok(expected), "{args:?}");
        }
    }

    #[test]
    fn historical_streams_keep_their_capture_and_codec() {
        for (command, index, target, codec) in [
            ("pattern", None, None, VideoCodec::Rgb332),
            (
                "capture-stream",
                Some("2"),
                Some(CaptureTarget::Monitor(2)),
                VideoCodec::Rgb332,
            ),
            (
                "h264-stream",
                Some("1"),
                Some(CaptureTarget::Monitor(1)),
                VideoCodec::H264,
            ),
            (
                "virtual-h264",
                None,
                Some(CaptureTarget::WindowDeck),
                VideoCodec::H264,
            ),
            (
                "auto-virtual-h264",
                None,
                Some(CaptureTarget::WindowDeckAuto),
                VideoCodec::H264,
            ),
        ] {
            for address in [None, Some("127.0.0.1:48151")] {
                let mut args = vec!["diag", command];
                args.extend(index);
                args.extend(address);
                assert_eq!(
                    parse(&args),
                    Ok(Mode::Serve {
                        address: address.unwrap_or(DEFAULT_ADDRESS).into(),
                        monitor: target,
                        codec,
                    })
                );
            }
        }
    }

    #[test]
    fn invalid_diagnostics_and_extra_arguments_never_start_a_session() {
        for args in [
            vec!["diag", "unknown"],
            vec!["diag", "capture", "0"],
            vec!["diag", "encode", "-1"],
            vec!["diag", "h264-stream"],
            vec!["diag", "capture-stream", "x"],
            vec!["diag", "gpu-encode", "h264_mf"],
            vec!["diag", "pattern", "--driver-h264"],
            vec!["127.0.0.1:48150"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
        for mut args in [
            vec!["--help"],
            vec!["--version"],
            vec!["--driver-h264", DEFAULT_ADDRESS],
            vec!["--driver-native-h264", DEFAULT_ADDRESS],
            vec!["diag", "--help"],
            vec!["diag", "capture", "1"],
            vec!["diag", "encode", "1"],
            vec!["diag", "driver-frames"],
            vec!["diag", "gpu-frames"],
            vec!["diag", "gpu-encode", "auto"],
            vec!["diag", "pattern", DEFAULT_ADDRESS],
            vec!["diag", "virtual-h264", DEFAULT_ADDRESS],
            vec!["diag", "auto-virtual-h264", DEFAULT_ADDRESS],
            vec!["diag", "h264-stream", "1", DEFAULT_ADDRESS],
            vec!["diag", "capture-stream", "1", DEFAULT_ADDRESS],
        ] {
            args.push("extra");
            assert!(parse(&args).is_err(), "{args:?}");
        }
        for old_flag in [
            "--capture-test",
            "--encode-test",
            "--driver-frame-test",
            "--gpu-frame-test",
            "--gpu-self-test",
            "--capture",
            "--h264",
            "--virtual-h264",
            "--auto-virtual-h264",
        ] {
            assert!(parse(&[old_flag]).unwrap_err().contains("diag"));
        }
    }
}
