# ToggLinux

An **unofficial** desktop client for [Toggl Track](https://toggl.com/track/) on Linux (Ubuntu/Debian focus).

> ⚠️ ToggLinux is not affiliated with, endorsed by, or sponsored by Toggl™. Toggl™ is a trademark of Toggl OÜ.
> ⚠️ Working name — will be renamed before any public release.

Built with **Tauri v2** (Rust + WebKitGTK) and **React + Tailwind**: a fast, browser-aesthetic timer app that lives in your tray, keeps running in the background, detects idle time (even on GNOME Wayland), and talks directly to the Toggl Track API.

## Features (planned)

- ▶ Start / stop / edit time entries with project, task and tag pickers
- 🕒 Today view, resume-last-entry, duplicate entry
- 🔧 Tray menu with live "today's total" + running-entry text
- 🖥 Daemon-like: closes to tray, single instance, autostart, `--hidden`
- 😴 Idle detection with split/discard dialog (X11, GNOME Wayland via Mutter, KDE)
- ⌨️ Global hotkeys (X11; Wayland via custom keybinding)
- 📊 Reports? We link you to the web app instead.

## Development

Prerequisites (Ubuntu 24.04+):

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf xdg-utils build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # if no cargo
```

```bash
npm install
npm run tauri dev      # dev window with hot reload
npm run tauri build    # .deb / .AppImage / .rpm in src-tauri/target/release/bundle
```

GNOME users without Ubuntu's shell need the **AppIndicator and KStatusNotifierItem** extension for the tray icon.

## Status

Early scaffold (M0): auth token storage, tray menu, hide-to-tray, placeholder timer. See the roadmap in commits/issues.

## License

TBD (MIT/Apache-2.0 recommended).
