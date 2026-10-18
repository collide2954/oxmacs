use jiff::Zoned;
use jiff::fmt::strtime;

const SECOND_DIRECTIVES: &[&str] = &["%S", "%T", "%r", "%X", "%s", "%f", "%c"];

pub fn text(format: &str, at: &Zoned) -> Result<String, String> {
    strtime::format(format, at).map_err(|error| error.to_string())
}

pub fn now(format: &str) -> String {
    text(format, &Zoned::now()).unwrap_or_default()
}

pub fn shows_seconds(format: &str) -> bool {
    SECOND_DIRECTIVES
        .iter()
        .any(|directive| format.contains(directive))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    use super::*;

    fn afternoon() -> Zoned {
        date(2026, 10, 18)
            .at(14, 5, 9, 0)
            .to_zoned(TimeZone::UTC)
            .unwrap()
    }

    #[test]
    fn clocks_follow_their_format() {
        let at = afternoon();

        assert_eq!(text("%H:%M", &at).unwrap(), "14:05");
        assert_eq!(text("%a %H:%M", &at).unwrap(), "Sun 14:05");
        assert_eq!(text("%b %d %H:%M", &at).unwrap(), "Oct 18 14:05");
        assert_eq!(text("%I:%M %p", &at).unwrap(), "02:05 PM");
    }

    #[test]
    fn bad_formats_are_errors() {
        assert!(text("%H %", &afternoon()).is_err());
    }

    #[test]
    fn formats_with_seconds_are_noticed() {
        assert!(shows_seconds("%H:%M:%S"));
        assert!(shows_seconds("%T"));
        assert!(!shows_seconds("%a %H:%M"));
    }
}
