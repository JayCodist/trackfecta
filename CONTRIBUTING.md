# Contributing to ToggLinux

Thanks for your interest in improving ToggLinux! This document explains how to set
up a development environment, how we work, and what a healthy pull request looks
like. All contributions are licensed under the project's [MIT License](LICENSE) —
by opening a PR you agree to that ("inbound = outbound"; there is no CLA).

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md) in all
interactions.

## Contents

- [Ways to contribute](#ways-to-contribute)
- [Setting up the development environment](#setting-up-the-development-environment)
- [Running the app](#running-the-app)
- [Checks before you push](#checks-before-you-push)
- [How we work](#how-we-work)
- [Code guidelines](#code-guidelines)
- [Architecture primer](#architecture-primer)
- [Asking for help](#asking-for-help)

## Ways to contribute

- **Report bugs** with the issue template — include your distro, desktop
  environment, session type (X11/Wayland) and app version.
- **Propose features** as enhancement issues. For anything larger than a quick
  win, open the issue *first* and reach agreement before writing code.
- **Pick up an issue.** Look for [`good first issue`](https://github.com/JayCodist/ToggLinux/labels/good%20first%20issue)
  or [`help wanted`](https://github.com/JayCodist/ToggLinux/labels/help%20wanted),
  and say you're taking it in the thread so effort isn't duplicated.
- **Review and test** open PRs, improve docs and translations, or help with
  packaging (Flatpak/Snap are great areas for contribution).

## Setting up the development environment

1. Install the system dependencies listed in the
   [README](README.md#building-from-source) (WebKitGTK 4.1, `libayatana-appindicator`,
   `pkg-config`, a C toolchain, `patchelf`, `xdg-utils`).
2. Install [rustup](https://rustup.rs) (stable toolchain) if you don't have cargo.
3. Install Node.js ≥ 20 (the repo uses npm and ships a `package-lock.json`).
4. Clone and install:

   ```bash
   git clone https://github.com/JayCodist/ToggLinux.git
   cd ToggLinux
   npm install
   ```

## Running the app

```bash
npm run tauri:dev
```

This starts Vite (port 1420) and a Tauri window with hot reload. The frontend is
TypeScript + Svelte 5 (runes) + Tailwind v4; the Rust side lives in `src-tauri/`.

To produce bundles locally:

```bash
npm run tauri:build   # .deb / .rpm / AppImage under src-tauri/target/release/bundle
```

## Checks before you push

CI runs these on every push and pull request — run them locally first:

```bash
npm run check                          # svelte-check + tsc
npm run build                          # frontend production build
cd src-tauri
cargo check
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Formatting and lint are non-negotiable (rustfmt and clippy defaults); keep new code
warning-free rather than adding `#[allow(...)]` unless you explain why.

## How we work

- **Branches:** create a topic branch from `main` (e.g. `fix/tray-label-encoding`).
- **Commits:** [Conventional Commits](https://www.conventionalcommits.org/) —
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`. Imperative subject,
  body explains *why*, not *how*.
- **One logical change per PR.** Split refactors from behaviour changes when you can.
- **Pull requests:** fill in the template, link the issue you're closing
  (`Closes #123`), and attach a screenshot/recording for UI changes.
- **Review:** a maintainer reviews; expect at least one approval and green CI
  before merge. We squash-merge topic branches to keep `main` history readable.
- **Release notes:** user-visible changes belong in [CHANGELOG.md](CHANGELOG.md) —
  add your entry under `Unreleased` in the same PR.

## Code guidelines

- **Rust:** rustfmt defaults; prefer `thiserror`-style typed errors over stringly
  errors in new modules; no `unwrap()` on fallible user/network paths — surface
  errors to the UI instead.
- **Svelte/TS:** Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`);
  `strict` TypeScript; no `any` without a comment justifying it. Note that a prop
  or variable must not be named `state` (it collides with the `$state` rune).
- **Styling:** Tailwind utilities first; shared visual tokens live in
  `src/styles.css` CSS variables so themes keep working.
- **Privacy:** never log, cache or transmit the API token outside the keyring and
  the Toggl API request path. No telemetry, ever, without an explicit opt-in issue
  and discussion first.
- **Trademarks:** do not add Toggl logos or brand assets; keep the unofficial
  disclaimer in user-visible places.

## Architecture primer

Rust owns state; the webview is a thin view.

- Commands go UI → Rust via Tauri `invoke()` (registered in `src-tauri/src/lib.rs`).
- State flows back as events via `app.emit(...)` (e.g. `timer-state`,
  `resume-requested`, `toast`), consumed in Svelte with `listen()`.
- `src-tauri/src/state.rs` holds the single app state; `state::broadcast()` pushes
  every change to the window **and** the tray — new state should go through it.
- `src-tauri/src/sync.rs` is the background poll loop; `toggl/` is the typed API
  client; `secrets.rs` is the only place that touches the keyring.
- The tray handles all background behaviour: close-to-tray, dynamic menu text,
  single-instance re-focus.
- New capabilities must be granted in `src-tauri/capabilities/` — a PR that adds a
  command or plugin without updating capabilities will not work at runtime.

When adding a feature, follow the existing slice: Rust command + emitted state +
typed mirror in `src/lib/timer.ts` (or a sibling module) + Svelte view.

## Asking for help

Open a discussion-style issue with the `question` label, or comment on the issue
you're working on. We'd rather answer early than review a missed assumption late.
