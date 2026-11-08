//! Paths, times and numbers the way the UI shows them.

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
    time.elapsed()
        .map(relative_age)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// How long `age` is: "just now", "5m ago", "3h ago", "2d ago".
pub fn relative_age(age: Duration) -> String {
    let secs = age.as_secs();
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

/// How long an agent has worked, the Thinking clock: whole seconds, "42s",
/// then "1:05" from a minute on; minutes keep counting past the hour.
pub fn clock(duration: Duration) -> String {
    match duration.as_secs() {
        secs @ 0..60 => format!("{secs}s"),
        secs => format!("{}:{:02}", secs / 60, secs % 60),
    }
}

/// How long something has run, as `mm:ss`, the way the build log keeps it;
/// minutes keep counting past the hour.
pub fn stopwatch(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// `12345` as `12,345`.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, c) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A file size: `512 B`, `12 kB`, `3.4 MB`.
pub fn bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.0} kB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
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
    fn the_clock_reads_seconds_then_minutes() {
        assert_eq!(clock(Duration::from_millis(4_900)), "4s");
        assert_eq!(clock(Duration::from_secs(59)), "59s");
        assert_eq!(clock(Duration::from_secs(60)), "1:00");
        assert_eq!(clock(Duration::from_secs(3_725)), "62:05");
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
        assert_eq!(relative_age(Duration::from_secs(90)), "1m ago");
    }

    #[test]
    fn paths_outside_home_stay_whole() {
        assert_eq!(display_path(Path::new("/opt/x")), "/opt/x");
    }
}
