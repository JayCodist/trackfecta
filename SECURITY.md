# Security Policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 1.x     | ✅        |
| < 1.0   | ❌        |

Security fixes are released for the current minor line. Users on older releases
should upgrade as part of any fix rollout.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Report them privately via GitHub's security advisory system:

👉 <https://github.com/JayCodist/ToggLinux/security/advisories/new>

You can expect:

- **acknowledgement** within 5 business days;
- a **triage/fix plan** within about 2 weeks, best effort (this is a volunteer
  project);
- a coordinated **advisory + release** once the fix is ready — we aim to give
  reporters credit in the advisory unless anonymity is requested.

Include as much of the following as you safely can: affected version, a proof of
concept or reproduction steps, and the impact. **Redact your real Toggl API
token** from any repro material.

## Scope worth reporting

- leakage or unsafe storage of the Toggl API token (it belongs in the OS keyring
  only);
- anything that lets the webview escape its CSP or capability grants;
- unsafe handling of data received from the Toggl Track API;
- privilege issues in the bundled `.desktop` / autostart entries.

## Out of scope

- physical/social attacks on a user's own machine or account;
- issues in Toggl's own servers (report those to Toggl);
- findings that require an already-compromised system or keyring.

## Relevant project facts

- ToggLinux stores the API token via the OS keyring (service `com.togglinux.app`)
  and never writes it to disk or logs.
- Network calls go only to `api.track.toggl.com` over HTTPS.
- The webview runs with a restrictive CSP and a minimal capability set
  (`src-tauri/capabilities/`).
- There is no telemetry.
