# Security Policy

## Supported versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a vulnerability

**Do not open a public issue.** Instead, open a
[private security advisory](https://github.com/stackwares/ubra-tauri/security/advisories/new)
or email `nemoryoliver@gmail.com`.

Include: what you did, what you expected, what happened, and the OS / app
version details if available. Expect an initial response within 7 days.

## Scope notes

Ubra **runs terminals locally**: panes spawn real shells and
coding-agent CLIs on your own machine with your own user privileges. That is
the documented trust model, not a vulnerability — running an untrusted command
in a pane is equivalent to running it in any other terminal. Reports about the
update surface, the auto-start/tray behavior, or the frontend/backend IPC
boundary are welcome. The app can also make provider usage requests and initialize
build-configured analytics; see [PRIVACY.md](PRIVACY.md) for network behavior.
