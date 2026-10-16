//! Paths and times the way the UI shows them.

use std::path::Path;
use std::time::{Duration, SystemTime};

/// A path with the home folder shortened to `~`.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = crate::settings::home()
        && let Ok(relative) = path.strip_prefix(&home)
    {
        return format!("~/{}", relative.display());
    }
    path.display().to_string()
}

/// How long ago `time` was: "just now", "5m ago", "3h ago", "2d ago".
pub fn relative_time(time: SystemTime) -> String {
    match time.elapsed() {
        Ok(duration) => {
            let secs = duration.as_secs();
            if secs < 60 {
                "just now".to_string()
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86_400 {
                format!("{}h ago", secs / 3600)
            } else {
                format!("{}d ago", secs / 86_400)
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

/// A date the way a card shows it: "today 18:02", "yesterday", "2 Oct".
pub fn card_date(time: SystemTime) -> String {
    let at = chrono::DateTime::<chrono::Local>::from(time);
    let date = at.date_naive();
    let today = chrono::Local::now().date_naive();
    if date == today {
        format!("today {}", at.format("%H:%M"))
    } else if Some(date) == today.pred_opt() {
        "yesterday".to_string()
    } else {
        at.format("%-d %b").to_string()
    }
}

/// How long something has run, as `mm:ss`; minutes keep counting past the
/// hour.
pub fn stopwatch(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stopwatch_reads_minutes_and_seconds() {
        assert_eq!(stopwatch(Duration::from_millis(4_900)), "00:04");
        assert_eq!(stopwatch(Duration::from_secs(125)), "02:05");
        assert_eq!(stopwatch(Duration::from_secs(3_725)), "62:05");
    }

    #[test]
    fn times_read_as_how_long_ago() {
        let ago = |secs| relative_time(SystemTime::now() - Duration::from_secs(secs));
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(90), "1m ago");
        assert_eq!(ago(2 * 3600 + 5), "2h ago");
        assert_eq!(ago(3 * 86_400), "3d ago");
        let later = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(relative_time(later), "unknown");
    }

    #[test]
    fn card_dates_name_today_and_yesterday() {
        assert!(card_date(SystemTime::now()).starts_with("today "));
        let day = SystemTime::now() - Duration::from_secs(86_400);
        assert_eq!(card_date(day), "yesterday");
    }

    #[test]
    fn paths_outside_home_stay_whole() {
        assert_eq!(display_path(Path::new("/opt/x")), "/opt/x");
    }
}
