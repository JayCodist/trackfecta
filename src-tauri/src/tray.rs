//! Dynamic tray icon. The panel icon is not a static logo. It is a live
//! readout of the time tracked today, with the official Toggl mark in front.
//! The mark shows whether a timer is logging: accent color when running, grey
//! when idle.
//!
//! Why the time is baked into the icon image: on GNOME/AppIndicator, the
//! panel does not render the item label. Only the icon shows. So the time has
//! to be drawn as pixels. The crate writes our RGBA to a temp PNG and points
//! the appindicator at it. `set_icon` is the update path.
//!
//! The glyphs are painted with the real desktop font through Pango/Cairo
//! (the same family as the shell clock) at 2x scale. That keeps the panel
//! downscale crisp. The earlier hand-made 5x7 bitmap font looked wrong next
//! to normal text. The mark is a baked-in alpha mask, rasterised once from
//! the official `mask-icon` SVG served by track.toggl.com, tinted per
//! logging state.

use std::sync::Mutex;
use std::time::Duration;

use cairo::{Context, Format, ImageSurface};
use tauri::image::Image;
use tauri::{AppHandle, Manager, Wry};

/// The tray icon id registered in `lib.rs`.
const TRAY_ID: &str = "main-tray";

/// The paint scale. We paint at 2x and let the shell downscale. That stays
/// crisp on HiDPI.
const K: u32 = 2;
/// The logical (1x) canvas height. Matches the GNOME panel icon slot.
const H1: u32 = 24;
/// Left padding before the mark, in logical px.
const PAD_L: u32 = 0;
/// The mark size at 1x. The baked 48 px mask is downscaled to this. Sized to
/// the cap height of the digits, so the mark neither dwarfs nor trails the
/// text.
const MARK1: u32 = 14;
/// Gap between the mark and the text, in logical px.
const GAP: u32 = 7;
/// Right padding, in logical px.
const PAD_R: u32 = 0;

/// The official Toggl mark: a 48x48 coverage mask (24 px at 2x), rasterised
/// once from track.toggl.com's `mask-icon` SVG and baked in. Then the tray
/// has no SVG or font dependencies at runtime. Values are alpha 0-255.
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

/// The font for the time readout: the desktop UI font (the same family the
/// shell clock uses), medium weight. Then the tray text matches everything
/// else.
fn font() -> pango::FontDescription {
    let mut fd = pango::FontDescription::new();
    fd.set_family("Ubuntu Sans");
    fd.set_weight(pango::Weight::Medium);
    // Tabular figures, so the ticking readout does not change width each
    // second.
    fd.set_variations(Some("tnum=1"));
    // The absolute size is in PANGO UNITS (x1024), not device px: 15 logical
    // px, doubled for the 2x canvas. (Passing raw px here resolved a font of
    // about size 0. That gave zero-width text, and the near-square canvas
    // read as an oval mark.)
    fd.set_absolute_size(12.0 * K as f64 * pango::SCALE as f64);
    fd
}

/// Renders `text` with a leading Toggl mark into an RGBA buffer. `running`
/// picks the color of the mark: accent while logging, muted grey when idle.
/// The text is white with a soft black shadow, so it reads on both panel
/// themes.
fn render(running: bool, text: &str) -> (Vec<u8>, u32, u32) {
    let h = H1 * K;

    // Measure the string first (probe surface), so the canvas fits it
    // exactly.
    let probe = ImageSurface::create(Format::ARgb32, 8, 8).expect("probe surface");
    let pcr = Context::new(&probe).expect("probe context");
    let pl = pangocairo::functions::create_layout(&pcr);
    pl.set_font_description(Some(&font()));
    pl.set_text(text);
    let (tw, th) = pl.pixel_size();
    let (tw, th) = (tw.max(0) as u32, th.max(0) as u32);

    let w = (PAD_L + MARK1 + GAP) * K + tw + PAD_R * K;
    let mut surf = ImageSurface::create(Format::ARgb32, w as i32, h as i32).expect("surface");
    let cr = Context::new(&surf).expect("context");

    // The mark: a premultiplied BGRA surface built from the baked alpha mask,
    // tinted by logging state. Accent when running, grey when idle.
    let (mr, mg, mb) = if running {
        (221u32, 56, 115) // accent; matches --accent in styles.css
    } else {
        (150u32, 150, 150) // idle grey
    };
    let mut mdata = vec![0u8; (48 * 48 * 4) as usize];
    for (i, &a) in MARK.iter().enumerate() {
        let a = a as u32;
        mdata[i * 4] = (mb * a / 255) as u8;
        mdata[i * 4 + 1] = (mg * a / 255) as u8;
        mdata[i * 4 + 2] = (mr * a / 255) as u8;
        mdata[i * 4 + 3] = a as u8;
    }
    let msurf =
        ImageSurface::create_for_data(mdata, Format::ARgb32, 48, 48, 48 * 4).expect("mark surface");
    let m1 = (MARK1 * K) as f64; // the mark edge, in device px
    let _ = cr.save();
    let _ = cr.translate((PAD_L * K) as f64, ((h - m1 as u32) / 2) as f64);
    let _ = cr.scale(m1 / 48.0, m1 / 48.0);
    cr.set_source_surface(&msurf, 0.0, 0.0).expect("mark source");
    let _ = cr.paint();
    let _ = cr.restore();

    // The time glyphs, centered vertically after the mark. First the shadow
    // pass, then white.
    let layout = pangocairo::functions::create_layout(&cr);
    layout.set_font_description(Some(&font()));
    layout.set_text(text);
    let tx = ((PAD_L + MARK1 + GAP) * K) as f64;
    let ty = ((h.saturating_sub(th)) / 2) as f64;
    let _ = cr.move_to(tx, ty + 1.0);
    let _ = pangocairo::functions::layout_path(&cr, &layout);
    let _ = cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    let _ = cr.fill();
    let _ = cr.move_to(tx, ty);
    let _ = pangocairo::functions::layout_path(&cr, &layout);
    let _ = cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.fill();

    // Cairo gives us premultiplied BGRA. tauri wants straight RGBA. Drop the
    // context and layout first. `data()` demands exclusive ownership.
    drop(layout);
    drop(cr);
    let stride = surf.stride() as usize;
    let mut rgba = vec![0u8; (w * h) as usize * 4];
    {
        let data = surf.data().expect("surface data");
        for y in 0..h as usize {
            for x in 0..w as usize {
                let s = y * stride + x * 4;
                let (b, g, r, a) = (data[s], data[s + 1], data[s + 2], data[s + 3]);
                let d = (y * w as usize + x) * 4;
                if a == 0 {
                    rgba[d..d + 4].copy_from_slice(&[0, 0, 0, 0]);
                } else {
                    let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                    rgba[d] = un(r);
                    rgba[d + 1] = un(g);
                    rgba[d + 2] = un(b);
                    rgba[d + 3] = a;
                }
            }
        }
    }

    // Trim the fully-transparent columns. Pango sizes the canvas from the
    // logical advance, which includes the side bearings. That is generous for
    // tabular figures. The round mark's mask is square. So the painted buffer
    // has invisible margins that the panel then stretches. Cropping to the
    // real ink makes the edges as tight as PAD_L and PAD_R intend.
    let mut minx = w;
    let mut maxx = 0u32;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 0 {
                if x < minx {
                    minx = x;
                }
                if x > maxx {
                    maxx = x;
                }
            }
        }
    }
    if minx <= maxx && (minx > 0 || maxx + 1 < w) {
        let cw = maxx - minx + 1;
        let mut out = vec![0u8; (cw * h) as usize * 4];
        for y in 0..h {
            let s = ((y * w + minx) * 4) as usize;
            let e = ((y * w + minx + cw) * 4) as usize;
            let d0 = ((y * cw) * 4) as usize;
            out[d0..d0 + (cw * 4) as usize].copy_from_slice(&rgba[s..e]);
        }
        return (out, cw, h);
    }
    (rgba, w, h)
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

    // GNOME renders no label in the panel when there is no menu open. So the
    // description of the running entry rides along as the hover tooltip.
    let tip = if running {
        format!("{}: {}", description.as_deref().unwrap_or("Untitled"), text)
    } else {
        format!("Today: {text}")
    };
    let _ = tray.set_tooltip(Some(&tip));

    let (rgba, w, h) = render(running, &text);
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
mod tmp_bbox {
    use super::*;
    #[test]
    fn bbox() {
        for (running, text) in [(true, "0:15:34"), (false, "2:01")] {
            let (rgba, w, h) = render(running, text);
            let (mut x0, mut x1, mut y0, mut y1) = (u32::MAX, 0u32, u32::MAX, 0u32);
            for y in 0..h {
                for x in 0..w {
                    if rgba[((y * w + x) * 4 + 3) as usize] > 8 {
                        x0 = x0.min(x);
                        x1 = x1.max(x);
                        y0 = y0.min(y);
                        y1 = y1.max(y);
                    }
                }
            }
            println!("MARKER {text:?} canvas={w}x{h} content x={x0}..{x1} y={y0}..{y1}");
        }
    }
}
