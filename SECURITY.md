# Security Policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 1.x     | Yes       |
| < 1.0   | No        |

Security fixes are released for the current minor line. Users on older releases
should upgrade as part of any fix rollout.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Report them privately through the GitHub security advisory system:

<https://github.com/JayCodist/trackfecta/security/advisories/new>

You can expect:

- an **acknowledgement** within 5 business days.
- a **triage and fix plan** within about 2 weeks, best effort. This is a
  volunteer project.
- a coordinated **advisory and release** once the fix is ready. We aim to give
  reporters credit in the advisory unless they ask for anonymity.

Include as much of the following as you safely can: the affected version, a
proof of concept or reproduction steps, and the impact. **Remove your real
Toggl API token** from any reproduction material.

## Scope worth reporting

- leakage or unsafe storage of the Toggl API token. It belongs only in the OS
  keyring.
- anything that lets the webview escape its CSP or capability grants.
- unsafe handling of data received from the Toggl Track API.
- privilege issues in the bundled `.desktop` or autostart entries.

## Out of scope

- physical or social attacks on a user's own machine or account.
- issues in Toggl's own servers. Report those to Toggl.
- findings that require an already-compromised system or keyring.

## Relevant project facts

- Trackfecta stores the API token via the OS keyring (service `com.trackfecta.app`)
  and never writes it to disk or logs.
- Network calls go only to `api.track.toggl.com` over HTTPS.
- The webview runs with a restrictive CSP and a minimal capability set
  (`src-tauri/capabilities/`).
- There is no telemetry.
