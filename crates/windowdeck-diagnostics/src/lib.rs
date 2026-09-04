use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        })
    }
}

pub fn emit(level: Level, event: &str, fields: &[(&str, &str)]) {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    eprintln!("{}", format_event(timestamp_ms, level, event, fields));
}

fn format_event(timestamp_ms: u128, level: Level, event: &str, fields: &[(&str, &str)]) -> String {
    let mut output = format!("timestamp_ms={timestamp_ms} level={level} event={event}");
    for (key, value) in fields {
        output.push(' ');
        output.push_str(key);
        output.push('=');
        output.push_str(&format!("{value:?}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_values_stay_on_one_log_line() {
        assert_eq!(
            format_event(
                42,
                Level::Warn,
                "session_closed",
                &[("error", "first line\nsecond line")],
            ),
            "timestamp_ms=42 level=WARN event=session_closed error=\"first line\\nsecond line\""
        );
    }
}
