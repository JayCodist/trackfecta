# Changelog

All notable changes to ToggLinux are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-10-01

First public release.

### Added

- Tray-first Toggl Track client for Linux built on Tauri v2 (Rust) with a
  Svelte 5 + Tailwind CSS v4 interface.
- Quick start/stop of time entries synced directly to the Toggl Track API v9,
  from both the window and the tray.
- Resume-last-entry from the tray menu, prefilled with the last description.
- Live tray status: the running entry while it ticks, today's total when idle.
- Background sync loop (10 s while running, 30 s idle, instant refresh on
  window focus) with rate-limit backoff honouring `429 + Retry-After`.
- API-token authentication validated against `GET /me`, stored in the OS
  keyring (no token on disk), with session restore on launch and graceful
  logout when a token is rejected.
- Daemon-like behaviour: close-to-tray, single instance, autostart.
- Light / dark / system theme via `prefers-color-scheme`.
- Release workflow builds `.deb`, `.rpm` and AppImage bundles for GitHub
  Releases (first published release coming soon).

[1.0.0]: https://github.com/JayCodist/ToggLinux/releases/tag/v1.0.0
