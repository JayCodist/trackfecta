# Contributing to TrackFecta

Thanks for your interest in improving TrackFecta. This document explains how to
set up a development environment, how we work, and what a good pull request
looks like. All contributions are licensed under the project's
[MIT License](LICENSE). When you open a PR, you agree to that. There is no CLA.

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

- **Report bugs** with the issue template. Include your distribution, desktop
  environment, session type (X11 or Wayland), and app version.
- **Propose features** as enhancement issues. For anything larger than a quick
  fix, open the issue first and reach agreement before you write code.
- **Pick up an issue.** Look for
  [`good first issue`](https://github.com/JayCodist/trackfecta/labels/good%20first%20issue)
  or [`help wanted`](https://github.com/JayCodist/trackfecta/labels/help%20wanted).
  Say that you are taking the issue in the thread, so effort is not
  duplicated.
- **Review and test** open PRs, improve the documentation and translations, or
  help with packaging. Flatpak and Snap are good areas for contribution.

## Setting up the development environment

1. Install the system dependencies listed in the
   [README](README.md#building-from-source): WebKitGTK 4.1,
   `libayatana-appindicator`, `pkg-config`, a C toolchain, `patchelf`, and
   `xdg-utils`.
2. Install [rustup](https://rustup.rs) (stable toolchain) if you do not have
   cargo.
3. Install Node.js 20 or newer. The repo uses npm and has a
   `package-lock.json`.
4. Clone and install:

   ```bash
   git clone https://github.com/JayCodist/trackfecta.git
   cd TrackFecta
   npm install
   ```

## Running the app

```bash
npm run tauri:dev
```

This starts Vite (port 1420) and a Tauri window with hot reload. The frontend
uses TypeScript, Svelte 5 (runes), and Tailwind v4. The Rust code is in
`src-tauri/`.

To produce bundles locally:

```bash
npm run tauri:build   # .deb, .rpm, AppImage under src-tauri/target/release/bundle
```

## Checks before you push

CI runs these on every push and pull request. Run them locally first:

```bash
npm run check                          # svelte-check and tsc
npm run build                          # frontend production build
cd src-tauri
cargo check
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Formatting and lint checks are required (rustfmt and clippy defaults). Keep
new code free of warnings instead of adding `#[allow(...)]`, unless you explain
why the allow is needed.

## How we work

- **Branches:** create a topic branch from `main`, for example
  `fix/tray-label-encoding`.
- **Commits:** use [Conventional Commits](https://www.conventionalcommits.org/):
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`. Use an imperative
  subject. The body explains why the change is made, not how.
- **One logical change per PR.** Split refactors from behavior changes when
  you can.
- **Pull requests:** fill in the template, link the issue you are closing
  (`Closes #123`), and attach a screenshot or a recording for UI changes.
- **Review:** a maintainer reviews the PR. Expect at least one approval and a
  passing CI run before merge. We squash-merge topic branches to keep the
  history of `main` readable.
- **Release notes:** put user-visible changes in
  [CHANGELOG.md](CHANGELOG.md). Add your entry under `Unreleased` in the same
  PR.

## Code guidelines

- **Rust:** use rustfmt defaults. Prefer typed errors (`thiserror`) over
  string errors in new modules. Do not use `unwrap()` on a path that can fail,
  such as user input or the network. Send those errors to the UI instead.
- **Svelte and TypeScript:** use Svelte 5 runes (`$state`, `$derived`,
  `$effect`, `$props`), `strict` TypeScript, and no `any` without a comment
  that justifies it. Note that a prop or variable must not be named `state`.
  That name collides with the `$state` rune.
- **Styling:** prefer Tailwind utilities. Shared visual tokens live in the CSS
  variables of `src/styles.css`, so themes keep working.
- **Privacy:** never log, cache, or transmit the API token outside the keyring
  and the Toggl API request path. No telemetry, ever, unless there is an
  explicit opt-in issue and a discussion first.
- **Trademarks:** do not add Toggl logos or brand assets. Keep the unofficial
  disclaimer in user-visible places.

## Architecture primer

Rust owns the state. The webview is a thin view.

- Commands go from the UI to Rust with the Tauri `invoke()` function. They are
  registered in `src-tauri/src/lib.rs`.
- State comes back as events with `app.emit(...)`, such as `timer-state`,
  `resume-requested`, and `toast`. The Svelte code consumes them with
  `listen()`.
- `src-tauri/src/state.rs` holds the single app state. `state::broadcast()`
  sends every change to the window and the tray. New state must go through it.
- `src-tauri/src/sync.rs` is the background poll loop. `toggl/` is the typed
  API client. `secrets.rs` is the only place that touches the keyring.
- The tray handles all background behavior: close-to-tray, dynamic menu text,
  and single-instance re-focus.
- New capabilities must be granted in `src-tauri/capabilities/`. A PR that
  adds a command or plugin without updating capabilities will not work at
  runtime.

When you add a feature, follow the existing pattern: a Rust command, emitted
state, a typed mirror in `src/lib/timer.ts` (or a sibling module), and the
Svelte view.

## Asking for help

Open a discussion-style issue with the `question` label, or comment on the
issue you are working on. We would rather answer early than review a missed
assumption late.
