use super::AnyError;
use windowdeck_diagnostics::{Level, emit};
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

struct CaptureProbe;

impl GraphicsCaptureApiHandler for CaptureProbe {
    type Flags = (String, usize);
    type Error = AnyError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        emit(
            Level::Info,
            "capture_started",
            &[
                ("monitor", &context.flags.1.to_string()),
                ("name", &context.flags.0),
            ],
        );
        Ok(Self)
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let _texture = frame.as_raw_texture();
        emit(
            Level::Info,
            "frame_captured",
            &[
                ("width", &frame.width().to_string()),
                ("height", &frame.height().to_string()),
                ("surface", "d3d11_texture"),
            ],
        );
        capture_control.stop();
        Ok(())
    }
}

pub fn run(index: usize) -> Result<(), AnyError> {
    let monitor = Monitor::from_index(index)?;
    let name = monitor.name()?;
    CaptureProbe::start(Settings::new(
        monitor,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (name, index),
    ))?;
    Ok(())
}
