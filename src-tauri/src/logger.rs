//! Minimal rolling file logger. This gives a paper trail for bug reports and
//! crash analysis. There is no crash-reporting service for this app. Logs
//! stay local.
//!
//! Layout: `<app_data>/logs/togglinux.log` is always today's log. On the
//! first write after midnight (UTC), the app renames it to
//! `togglinux-YYYY-MM-DD.log` and starts a fresh file. Dated files older than
//! `RETENTION_DAYS` are deleted during the rename. A per-day line cap limits
//! the worst case (a constant-error state with a raised hourly request cap)
//! to about 1 MB per day, regardless of churn.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;

/// Keep one month of history. Measured at 0.1 to 0.5 MB per day, so a few MB
/// worst case.
const RETENTION_DAYS: i64 = 31;
/// Hard cap of log lines per day-file. Measured at about 54 bytes per line,
/// so about 1 MB per day maximum.
const MAX_DAY_LINES: usize = 20_000;

struct LogState {
    dir: PathBuf,
    file: Option<File>,
    /// The UTC date (YYYY-MM-DD) that the open file belongs to.
    date: String,
    /// Lines written to the current day-file. Used for the cap.
    lines: usize,
}

static LOG: OnceLock<Mutex<Option<LogState>>> = OnceLock::new();

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn current_file(dir: &Path) -> PathBuf {
    dir.join("togglinux.log")
}

/// Opens the log, or rolls it over. setup calls this once, before anything
/// else logs. All failures are swallowed. Logging must never break the app.
pub fn init(data_dir: &std::path::Path) {
    let dir = data_dir.join("logs");
    let _ = fs::create_dir_all(&dir);
    let date = today();
    // If the file from a previous run belongs to an earlier day, roll it over.
    // Then delete the old files.
    rotate(&dir, &date);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(current_file(&dir))
        .ok();
    let _ = LOG.set(Mutex::new(Some(LogState {
        dir,
        file,
        date,
        lines: 0,
    })));
    log(
        "info",
        &format!("--- ToggLinux {} started ---", env!("CARGO_PKG_VERSION")),
    );
}

/// Renames `togglinux.log` to its dated name when it belongs to an earlier
/// day, and deletes dated files past the retention window.
fn rotate(dir: &Path, date: &str) {
    let cur = current_file(dir);
    if let Ok(meta) = fs::metadata(&cur) {
        let mtime_day = meta
            .modified()
            .ok()
            .map(|t| {
                chrono::DateTime::<chrono::Utc>::from(t)
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|| date.to_string());
        if mtime_day.as_str() < date {
            let dated = dir.join(format!("togglinux-{mtime_day}.log"));
            if dated.exists() {
                // The target of a previous rollover exists. Merge into it.
                if let (Ok(mut old), Ok(content)) = (
                    OpenOptions::new().append(true).open(&dated),
                    fs::read(&cur),
                ) {
                    let _ = old.write_all(&content);
                    let _ = fs::remove_file(&cur);
                }
            } else {
                let _ = fs::rename(&cur, &dated);
            }
        }
    }
    prune(dir, date);
}

/// Deletes `togglinux-YYYY-MM-DD.log` files older than the retention window.
fn prune(dir: &Path, date: &str) {
    let cutoff = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|d| d - chrono::Duration::days(RETENTION_DAYS));
    let Some(cutoff) = cutoff else { return };
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(day) = name
            .strip_prefix("togglinux-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        let old = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map(|d| d < cutoff)
            .unwrap_or(false);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Appends one line, formatted as `2026-10-03T01:13:49Z [level] message`.
/// Handles the midnight rollover and the per-day cap.
pub fn log(level: &str, msg: &str) {
    let Some(m) = LOG.get() else { return };
    let Ok(mut guard) = m.lock() else { return };
    let Some(st) = guard.as_mut() else { return };
    let date = today();
    if date != st.date {
        // It is midnight (UTC). Close, roll over, and reopen.
        st.file = None;
        rotate(&st.dir, &date);
        st.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(current_file(&st.dir))
            .ok();
        st.date = date;
        st.lines = 0;
    }
    if st.lines >= MAX_DAY_LINES {
        // The cap for the day is reached. Drop the line. The file stays
        // bounded.
        return;
    }
    st.lines += 1;
    if let Some(f) = st.file.as_mut() {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let _ = writeln!(f, "{now} [{level}] {msg}");
        let _ = f.flush();
    }
}

/// Gives the path of the log for today. The Settings screen shows it.
pub fn file_path() -> Option<PathBuf> {
    LOG.get()
        .and_then(|m| m.lock().ok())
        .and_then(|s| s.as_ref().map(|st| current_file(&st.dir)))
}

/// Gives the tail of today's log, for the "Report a problem" view.
pub fn read_tail(max_chars: usize) -> String {
    let Some(p) = file_path() else {
        return String::new();
    };
    fs::read_to_string(p)
        .map(|s| {
            let mut start = s.len().saturating_sub(max_chars);
            // Do not cut a line in half. Advance to the next newline boundary.
            while start < s.len() && !s.is_char_boundary(start) {
                start += 1;
            }
            match s[start..].find('\n') {
                Some(i) if start + i + 1 < s.len() => s[start + i + 1..].to_string(),
                _ => s[start..].to_string(),
            }
        })
        .unwrap_or_default()
}

/// Captures panics (the sync task, command handlers) into the log, so a crash
/// can still be explained after the fact. The default hook still runs after.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log("panic", &info.to_string());
        prev(info);
    }));
}
