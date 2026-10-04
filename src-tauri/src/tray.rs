//! Dynamic tray item: the official Toggl mark plus a live readout of the
//! time tracked today. The mark shows whether a timer is logging: accent
//! color when running, grey when idle.
//!
//! This is the conventional appindicator layout, the same one Slack uses
//! (its unread badge is an icon plus a native text label):
//!
//!   * `set_icon` keeps the square mark. The panel clamps square icons to
//!     the standard icon slot and scales them with its own image pipeline.
//!   * `set_title` sets the appindicator `XAyatanaLabel` property. The
//!     Ubuntu appindicator extension renders it as a real `St.Label` next
//!     to the icon, in the panel font: crisp, themed, and spaced identically
//!     to every other tray app, for free.
//!
//! The time is deliberately NOT baked into the icon pixels. Drawing glyphs
//! ourselves means a fixed raster size (soft next to hinted shell text) and
//! a wide image that trips the panel's aspect-preserving squeeze (the
//! "indicator-multiload" path), which manufactures transparent side margins
//! that read as oversized padding. Both problems are invisible to the icon
//! plus native-label approach because the shell owns the text.

use std::sync::Mutex;
use std::time::Duration;

use tauri::image::Image;
use tauri::{AppHandle, Manager, Wry};

/// The tray icon id registered in `lib.rs`.
const TRAY_ID: &str = "main-tray";

/// The official Toggl mark: a 48x48 coverage mask, rasterised once from
/// track.toggl.com's `mask-icon` SVG and baked in, so the tray has no SVG
/// dependency at runtime. Values are alpha 0-255; the flat tint is applied
/// per logging state.
const MARK: [u8; 48 * 48] = [
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,22,87,143,183,217,238,249,250,238,218,185,146,91,26,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,17,106,200,254,255,255,255,255,255,255,255,255,255,255,255,255,255,206,113,22,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,24,139,243,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,247,149,31,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,4,111,241,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,247,128,8,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,33,202,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,213,43,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,74,239,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,245,88,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,91,250,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,254,115,0,0,0,0,0,0,0,
    0,0,0,0,0,0,93,254,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,110,0,0,0,0,0,0,
    0,0,0,0,0,73,250,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,253,85,0,0,0,0,0,
    0,0,0,0,32,238,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,156,119,119,156,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,244,40,0,0,0,0,
    0,0,0,3,199,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,209,6,0,0,0,
    0,0,0,110,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,119,0,0,0,
    0,0,22,240,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,244,26,0,0,
    0,0,135,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,217,133,255,69,0,0,69,255,154,199,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,142,0,0,
    0,15,241,255,255,255,255,255,255,255,255,255,255,255,255,255,255,221,86,1,7,255,69,0,0,69,255,63,0,57,198,255,255,255,255,255,255,255,255,255,255,255,255,255,255,244,18,0,
    0,102,255,255,255,255,255,255,255,255,255,255,255,255,255,255,176,15,0,0,7,255,69,0,0,69,255,63,0,0,1,119,253,255,255,255,255,255,255,255,255,255,255,255,255,255,106,0,
    0,195,255,255,255,255,255,255,255,255,255,255,255,255,255,155,1,0,0,0,21,255,69,0,0,69,255,76,0,0,0,0,102,254,255,255,255,255,255,255,255,255,255,255,255,255,198,0,
    19,253,255,255,255,255,255,255,255,255,255,255,255,255,185,3,0,0,8,132,243,255,69,0,0,69,255,251,154,15,0,0,0,133,255,255,255,255,255,255,255,255,255,255,255,255,254,20,
    82,255,255,255,255,255,255,255,255,255,255,255,255,240,28,0,0,16,196,255,255,255,69,0,0,69,255,255,255,215,28,0,0,6,210,255,255,255,255,255,255,255,255,255,255,255,255,84,
    140,255,255,255,255,255,255,255,255,255,255,255,255,128,0,0,1,183,255,255,255,255,69,0,0,69,255,255,255,255,209,10,0,0,75,255,255,255,255,255,255,255,255,255,255,255,255,141,
    180,255,255,255,255,255,255,255,255,255,255,255,250,22,0,0,95,255,255,255,255,255,69,0,0,69,255,255,255,255,255,134,0,0,1,216,255,255,255,255,255,255,255,255,255,255,255,180,
    217,255,255,255,255,255,255,255,255,255,255,255,194,0,0,0,213,255,255,255,255,255,69,0,0,69,255,255,255,255,255,243,15,0,0,139,255,255,255,255,255,255,255,255,255,255,255,216,
    235,255,255,255,255,255,255,255,255,255,255,255,133,0,0,40,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,89,0,0,78,255,255,255,255,255,255,255,255,255,255,255,234,
    247,255,255,255,255,255,255,255,255,255,255,255,105,0,0,88,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,142,0,0,49,255,255,255,255,255,255,255,255,255,255,255,247,
    250,255,255,255,255,255,255,255,255,255,255,255,88,0,0,103,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,160,0,0,32,255,255,255,255,255,255,255,255,255,255,255,250,
    238,255,255,255,255,255,255,255,255,255,255,255,107,0,0,85,255,255,255,255,255,255,69,0,0,69,255,255,255,255,255,255,140,0,0,53,255,255,255,255,255,255,255,255,255,255,255,237,
    222,255,255,255,255,255,255,255,255,255,255,255,132,0,0,47,255,255,255,255,255,255,243,238,238,243,255,255,255,255,255,255,100,0,0,79,255,255,255,255,255,255,255,255,255,255,255,221,
    186,255,255,255,255,255,255,255,255,255,255,255,189,0,0,1,227,255,255,255,255,255,255,255,255,255,255,255,255,255,255,252,23,0,0,138,255,255,255,255,255,255,255,255,255,255,255,186,
    146,255,255,255,255,255,255,255,255,255,255,255,250,17,0,0,116,255,255,255,255,255,255,255,255,255,255,255,255,255,255,158,0,0,0,219,255,255,255,255,255,255,255,255,255,255,255,147,
    91,255,255,255,255,255,255,255,255,255,255,255,255,113,0,0,9,213,255,255,255,255,255,255,255,255,255,255,255,255,232,26,0,0,68,255,255,255,255,255,255,255,255,255,255,255,255,92,
    25,254,255,255,255,255,255,255,255,255,255,255,255,235,14,0,0,38,232,255,255,255,255,255,255,255,255,255,255,241,58,0,0,2,207,255,255,255,255,255,255,255,255,255,255,255,255,26,
    0,204,255,255,255,255,255,255,255,255,255,255,255,255,157,0,0,0,32,203,255,255,255,255,255,255,255,255,210,42,0,0,0,122,255,255,255,255,255,255,255,255,255,255,255,255,206,0,
    0,110,255,255,255,255,255,255,255,255,255,255,255,255,255,108,0,0,0,2,90,185,249,255,255,249,186,94,3,0,0,0,80,253,255,255,255,255,255,255,255,255,255,255,255,255,114,0,
    0,20,245,255,255,255,255,255,255,255,255,255,255,255,255,253,120,0,0,0,0,0,1,24,24,1,0,0,0,0,0,101,249,255,255,255,255,255,255,255,255,255,255,255,255,247,23,0,
    0,0,143,255,255,255,255,255,255,255,255,255,255,255,255,255,255,172,31,0,0,0,0,0,0,0,0,0,0,26,161,255,255,255,255,255,255,255,255,255,255,255,255,255,255,150,0,0,
    0,0,26,243,255,255,255,255,255,255,255,255,255,255,255,255,255,255,247,145,59,2,0,0,0,0,2,57,142,244,255,255,255,255,255,255,255,255,255,255,255,255,255,255,246,31,0,0,
    0,0,0,116,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,236,198,172,172,197,235,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,126,0,0,0,
    0,0,0,5,205,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,213,8,0,0,0,
    0,0,0,0,35,240,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,247,47,0,0,0,0,
    0,0,0,0,0,77,251,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,254,90,0,0,0,0,0,
    0,0,0,0,0,0,92,254,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,115,0,0,0,0,0,0,
    0,0,0,0,0,0,0,94,250,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,254,119,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,75,239,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,245,89,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,33,201,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,213,43,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,2,102,240,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,246,128,8,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,22,137,242,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,246,147,29,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,16,104,197,254,255,255,255,255,255,255,255,255,255,255,255,255,255,203,111,22,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,20,85,141,182,216,236,249,249,237,216,183,143,88,23,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

/// The mark as a flat-tinted, straight-RGBA square: the baked alpha mask
/// colored by logging state. Accent while logging, muted grey when idle.
/// No text here; the time rides on the native label (`set_title`).
fn render_mark(running: bool) -> (Vec<u8>, u32, u32) {
    let (r, g, b) = if running {
        (221u8, 56, 115) // accent; matches --accent in styles.css
    } else {
        (150u8, 150, 150) // idle grey
    };
    let mut rgba = vec![0u8; MARK.len() * 4];
    for (i, &a) in MARK.iter().enumerate() {
        let d = i * 4;
        rgba[d] = r;
        rgba[d + 1] = g;
        rgba[d + 2] = b;
        rgba[d + 3] = a;
    }
    (rgba, 48, 48)
}

/// The day-time readout: `H:MM`, or `H:MM:SS` when `secs` is on.
fn format_total(total: i64, secs: bool) -> String {
    let total = total.max(0);
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if secs {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{h}:{m:02}")
    }
}

/// The last rendered signature (the running flag and the string). Skip
/// re-encoding when the visible icon would be identical.
static LAST: Mutex<Option<(bool, String)>> = Mutex::new(None);

/// Recomputes the time tracked today (adding the live elapsed time of the
/// running entry) and repaints the tray icon. This is cheap: it reads the
/// timer snapshot and one setting.
pub fn refresh(app: &AppHandle<Wry>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (running, started_at, description, today_seconds) = {
        let st = app.state::<crate::state::AppState>();
        let t = st.timer.lock().unwrap();
        (
            t.running,
            t.started_at.clone(),
            t.description.clone(),
            t.today_seconds,
        )
    };
    let secs = app
        .state::<crate::state::AppState>()
        .store
        .as_ref()
        .and_then(|s| s.get_setting("tray_show_seconds"))
        .map(|v| v == "true")
        .unwrap_or(false);

    let elapsed = if running {
        started_at
            .and_then(|s| s.parse::<i64>().ok())
            .map(|start| chrono::Utc::now().timestamp() - start)
            .unwrap_or(0)
    } else {
        0
    };
    let total = today_seconds + elapsed.max(0);
    let text = format_total(total, secs);

    // Only re-render when the visible result changed. The ticker fires often.
    {
        let mut last = LAST.lock().unwrap();
        if last.as_ref().is_some_and(|(r, t)| *r == running && *t == text) {
            return;
        }
        *last = Some((running, text.clone()));
    }

    // Hover tooltip: the running entry's description plus the total. The
    // crate's GTK backend no-ops this on Linux, but it costs nothing and
    // serves the other platforms.
    let tip = if running {
        format!("{}: {}", description.as_deref().unwrap_or("Untitled"), text)
    } else {
        format!("Today: {text}")
    };
    let _ = tray.set_tooltip(Some(&tip));

    // The time as a native label: the appindicator XAyatanaLabel property,
    // drawn by the shell as a real St.Label in the panel font. Crisp text,
    // themed color, and the same icon-to-text spacing every other tray app
    // gets for free. This is how Slack renders its unread badge.
    let _ = tray.set_title(Some(&text));

    let (rgba, w, h) = render_mark(running);
    let _ = tray.set_icon(Some(Image::new(&rgba, w, h)));
}

/// The adaptive repaint loop. It keeps the running time ticking without a
/// server poll:
///   * Running with seconds shown: 1 second.
///   * Running, no seconds: 20 seconds. This catches minute rollovers.
///   * Idle: 60 seconds. This is a cheap safety net. Data changes already
///     repaint through `broadcast`.
pub fn spawn(app: AppHandle<Wry>) {
    tauri::async_runtime::spawn(async move {
        loop {
            let st = app.state::<crate::state::AppState>();
            let running = st.timer.lock().unwrap().running;
            let secs = st
                .store
                .as_ref()
                .and_then(|s| s.get_setting("tray_show_seconds"))
                .map(|v| v == "true")
                .unwrap_or(false);
            drop(st);
            let wait = if running && secs {
                1
            } else if running {
                20
            } else {
                60
            };
            tokio::time::sleep(Duration::from_secs(wait)).await;
            refresh(&app);
        }
    });
}

#[cfg(test)]
mod render_tests {
    use super::*;

    // The mark must stay square: a square icon is clamped to the panel's
    // icon slot and never enters the wide-image path that gets squeezed and
    // grows transparent side margins. All text lives in the native label.
    #[test]
    fn mark_is_square_and_tinted() {
        for running in [true, false] {
            let (rgba, w, h) = render_mark(running);
            assert_eq!(w, h, "the mark must be square");
            assert_eq!(rgba.len(), (w * h * 4) as usize);
            let alpha = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
            assert_eq!(alpha(0, 0), 0, "corner must be transparent");
            // The mark is a ring: its exact center is hollow. Check that the
            // ring itself is drawn: the left edge at mid-height carries ink.
            assert!(alpha(1, 24) > 0, "the ring edge must be drawn");
            let inked = (0..w * h).filter(|i| rgba[(*i * 4 + 3) as usize] > 0).count();
            assert!(inked > 500, "the mark must be substantially drawn, got {inked}");
            // Straight RGBA: the tint is stored un-premultiplied.
            if running {
                let i = (24 * w + 24) as usize * 4;
                assert_eq!(&rgba[i..i + 3], &[221, 56, 115]);
            }
        }
    }
}
