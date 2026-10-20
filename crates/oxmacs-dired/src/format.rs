use jiff::{SignedDuration, Zoned};

const UNITS: [char; 6] = ['K', 'M', 'G', 'T', 'P', 'E'];
const SIX_MONTHS: SignedDuration = SignedDuration::from_secs(15_778_476);

pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes}B");
    }
    let bytes = u128::from(bytes);
    let mut unit = 1;
    let mut shown = String::new();
    for suffix in UNITS {
        unit *= 1024;
        let tenths = (bytes * 10).div_ceil(unit);
        let whole = bytes.div_ceil(unit);
        if tenths < 100 {
            return format!("{}.{}{suffix}", tenths / 10, tenths % 10);
        }
        shown = format!("{whole}{suffix}");
        if whole < 1024 {
            break;
        }
    }
    shown
}

pub fn ls_time(modified: &Zoned, now: &Zoned) -> String {
    let age = now.timestamp().duration_since(modified.timestamp());
    let format = if age.is_negative() || age > SIX_MONTHS {
        "%b %e  %Y"
    } else {
        "%b %e %H:%M"
    };
    modified.strftime(format).to_string()
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use jiff::tz::TimeZone;

    use super::*;

    #[test]
    fn small_sizes_are_shown_in_bytes() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(8), "8B");
        assert_eq!(human_size(1023), "1023B");
    }

    #[test]
    fn larger_sizes_round_up_like_ls_h() {
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1124), "1.1K");
        assert_eq!(human_size(6301), "6.2K");
        assert_eq!(human_size(10 * 1024), "10K");
        assert_eq!(human_size(10 * 1024 + 1), "11K");
        assert_eq!(human_size(1536 * 1024), "1.5M");
        assert_eq!(human_size(1024 * 1024 - 1), "1.0M");
        assert_eq!(human_size(5 * 1024_u64.pow(4)), "5.0T");
        assert_eq!(human_size(u64::MAX), "16E");
    }

    fn zoned(text: &str) -> Zoned {
        text.parse::<Timestamp>().unwrap().to_zoned(TimeZone::UTC)
    }

    #[test]
    fn recent_times_show_the_clock() {
        let now = zoned("2026-10-18T15:00:00Z");

        assert_eq!(
            ls_time(&zoned("2026-10-18T14:02:00Z"), &now),
            "Oct 18 14:02"
        );
        assert_eq!(
            ls_time(&zoned("2026-05-01T09:05:00Z"), &now),
            "May  1 09:05"
        );
    }

    #[test]
    fn old_and_future_times_show_the_year() {
        let now = zoned("2026-10-18T15:00:00Z");

        assert_eq!(
            ls_time(&zoned("2025-10-18T14:02:00Z"), &now),
            "Oct 18  2025"
        );
        assert_eq!(
            ls_time(&zoned("2027-01-01T00:00:00Z"), &now),
            "Jan  1  2027"
        );
    }
}
