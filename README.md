<div align="center">

# ToggLinux

**An unofficial tray-first Toggl Track desktop client for Linux.**

Fast, low-memory time tracking that lives in your system tray —
built with **Tauri v2** (Rust) and **Svelte 5** + **Tailwind CSS v4**.

[Features](#features) • [Install](#install) • [Getting started](#getting-started) •
[Building from source](#building-from-source) • [Contributing](#contributing)

![CI](https://github.com/JayCodist/ToggLinux/actions/workflows/ci.yml/badge.svg)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

</div>

> **Disclaimer**
> ToggLinux is an independent, unofficial client. It is **not affiliated with,
> endorsed by, or sponsored by Toggl OÜ**. Toggl™ is a trademark of Toggl OÜ.
> ToggLinux talks to the public Toggl Track REST API with *your own* API token and
> never sends your data anywhere except `api.track.toggl.com`.

## Why another client?

The official desktop apps do not run on Linux. ToggLinux is a native Rust process
with a WebKitGTK view — it starts instantly, idles at a fraction of the memory, and
behaves like a proper Linux citizen:

- runs from the **system tray** and keeps the timer ticking in the background
- **closes to tray** instead of quitting, so an accidental `X` never stops your day
- **single instance** — launching again just surfaces the existing window
- **autostart** entry, so the tray is there when you log in
- talks **directly to the Toggl Track API** — no Electron, no bundled web app
- your token lives in the **OS keyring**, never in a config file

## Features

- ▶️ **One-click quick timer** — start and stop entries synced straight to Toggl
  Track (window or tray)
- 🔁 **Resume last entry** from the tray menu, prefilled with the last description
- 🕒 **Live tray status** — the running entry while it ticks, today's total when idle
- 🧮 **Today's tracked time** in the window, kept fresh by a background sync loop
  (10 s while running, 30 s idle, instant refresh on window focus)
- 🔐 **API-token auth** validated against `GET /me`, stored in the OS keyring, with
  automatic session restore on launch and graceful logout when a token is rejected
- 🚦 **Rate-limit aware**: honours Toggl's `429 + Retry-After` instead of hammering
  the API
- 🖥 **Daemon-like**: close-to-tray, single instance, autostart
- 🌗 Light / dark / system theme via `prefers-color-scheme`
- 📦 **`.deb`, `.rpm` and AppImage** packaging via the release pipeline
  (prebuilt downloads coming soon)

Time *reports* are intentionally out of scope — use the web app for reporting,
exactly like the official desktop clients do.

## Install

Prebuilt packages are **coming soon**. The first published release will offer
`.deb`, `.rpm` and AppImage assets here with per-distro install commands.
Until then, please [build from source](#building-from-source) — ToggLinux is a
small native app and compiles in a few minutes.

Requirements for any install method: a GTK 3 desktop with
`libayatana-appindicator` (Ubuntu and most spins ship this) and WebKitGTK 4.1.
Target baseline is **Ubuntu 24.04+**, best effort on 22.04.

### GNOME tray icon

Stock GNOME does not ship a StatusNotifier host. Install the
**AppIndicator and KStatusNotifierItem Support** extension
([extensions.gnome.org](https://extensions.gnome.org/extension/615/appindicator-support/))
to make the tray icon appear. Ubuntu's GNOME fork enables this out of the box.

### Flatpak / Snap

Not offered yet. Packaging for Flatpak and Snap is a welcome contribution —
correctly self-hosting the tray (StatusNotifier) and the keyring inside a sandbox
needs validation first. See the
[issue tracker](https://github.com/JayCodist/ToggLinux/issues) and
[CONTRIBUTING.md](CONTRIBUTING.md).

## Getting started

1. Launch ToggLinux. The first screen asks for your **Toggl Track API token**,
   available on your [profile page](https://track.toggl.com/profile).
2. The token is written to your **system keyring** (service `com.togglinux.app`) and
   is never stored on disk in plain text. Your session is restored automatically on
   every launch.
3. Hit **Start** — a quick entry begins and the tray item shows it ticking. **Stop**
   from the window or the tray; the tray falls back to showing today's total.
4. Next day, pick **Resume: …** in the tray to restart the last entry.

Nothing leaves your machine except authenticated calls to the Toggl Track API.

## Building from source

### System dependencies

```bash
# Ubuntu / Debian
sudo apt install pkg-config build-essential libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf xdg-utils curl file
```

```bash
# Fedora
sudo dnf install pkg-config gcc gcc-c++ webkit2gtk4.1-devel \
  libayatana-appindicator3-devel librsvg2-devel patchelf xdg-utils curl file
```

Rust comes from [rustup](https://rustup.rs) (stable), Node.js 20 or newer.

### Develop

```bash
npm install
npm run tauri:dev      # Tauri window with hot reload (Vite on port 1420)
```

Useful checks:

```bash
npm run check          # svelte-check + TypeScript
npm run build          # type-check and build the frontend bundle
cd src-tauri && cargo check
cargo fmt --check && cargo clippy -- -D warnings
```

### Release build

```bash
npm run tauri:build    # .deb / .rpm / AppImage under src-tauri/target/release/bundle
```

## Project layout

```
src/                       Svelte 5 UI
  App.svelte               auth gate + view routing
  lib/Auth.svelte          first-run token screen
  lib/Timer.svelte         timer card
  lib/timer.ts             shared types mirroring the Rust state
src-tauri/src/
  lib.rs                   builder, commands, tray menu, startup
  state.rs                 app state, broadcasts, tray updates
  secrets.rs               keyring access for the API token
  sync.rs                  background polling loop
  toggl/                   Toggl Track API v9 client
  main.rs                  binary entrypoint
src-tauri/tauri.conf.json  window, CSP, bundle targets
src-tauri/capabilities/    webview permission grants
```

**Architecture in one line:** Rust owns all state (session, timer, tray, polling)
and the webview is a thin view. Commands go down via `invoke()`, state comes back
up as `emit()` events such as `timer-state`.

## Troubleshooting

| Symptom | Fix |
|---|---|
| No tray icon on GNOME | install the AppIndicator extension (see above) |
| Token screen reappears with a saved token | the keyring daemon isn't running or the login collection is locked; start GNOME Keyring / KWallet and unlock it |
| "token rejected" toast | the token was revoked or expired — paste a fresh one from your Toggl profile |
| Entries look stale | the app refreshes on window focus; check network — the sync loop retries automatically |

## Contributing

Bug reports, feature requests and pull requests are very welcome — start with
[CONTRIBUTING.md](CONTRIBUTING.md) and please read
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Good first issues are labelled
[`good first issue`](https://github.com/JayCodist/ToggLinux/issues?q=label%3A%22good+first+issue%22).

To report a **security** issue, see [SECURITY.md](SECURITY.md) — please do not open
a public issue for it.

## License

Released under the [MIT License](LICENSE).

Copyright (c) 2026 ToggLinux contributors.

Third-party trademarks and logos remain the property of their owners; this project
ships no Toggl brand assets.
