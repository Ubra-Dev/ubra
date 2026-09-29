# Ubra

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)](https://github.com/stackwares/ubra-tauri/releases)
[![Tauri](https://img.shields.io/badge/desktop-Tauri%20v2-ffc131.svg)](https://v2.tauri.app/)
[![Svelte](https://img.shields.io/badge/web-Svelte%205-ff3e00.svg)](https://svelte.dev/)

An agent runtime as a cross-platform desktop app (macOS, Linux, Windows).
Workspaces → tabs → terminal panes running real coding-agent CLIs, with agent
state badges, layout persistence, and agents that keep running when the window closes.

Built with [Tauri v2](https://v2.tauri.app/) (Rust backend), Svelte + TypeScript,
and [xterm.js](https://xtermjs.org/) for terminal rendering.

## Features

- **Multiplexer model** — workspaces containing tabs containing split terminal panes,
  with draggable dividers, pane zoom, and per-node rename.
- **Layout persistence** — window size/position and the full workspace layout
  restore across relaunches.
- **Tray behavior** — closing the window hides the app to the tray; agents keep
  running and the tray menu brings it back.
- **Agent awareness** — panes running agent CLIs (`claude`, `codex`, `opencode`, …)
  surface working/finished badges in pane headers, tabs, and the sidebar, plus a
  notification and sound when an agent finishes while you're elsewhere.
- **Notifications & sounds** — finished-agent alerts delivered as a system
  notification, an in-app toast, or off, with done/needs-attention chimes
  (custom sound file and per-agent muting supported) that play even when the
  window is hidden to the tray.

## Install

Download the latest bundle for your OS from
[Releases](https://github.com/stackwares/ubra-tauri/releases).

Or build from source:

```sh
npm install
npm run tauri build
```

Bundles land in `src-tauri/target/release/bundle/`.

## Prerequisites

- Rust via [rustup](https://rustup.rs/) + OS deps from the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- Node.js 20+ and npm

Note: this workspace vendors a local Rust toolchain under `.tooling/` (gitignored)
for sandboxed builds. On your own machine, install Rust normally and ignore that
directory (or `source .tooling/env.sh` from the repo root to reuse it here).

## Develop

```sh
npm install
npm run tauri dev
```

## Checks

```sh
npm run check                    # svelte-check
npm run test:unit                # frontend unit tests (node:test)
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo fmt --check
```

Run the full set before pushing. (The `CI` workflow in
`.github/workflows/` is currently disabled; re-enable it when ready.)

## Verifying manually

```sh
npm run tauri dev
```

- Split panes from the hover toolbar, drag the dividers, add tabs/workspaces,
  then quit and relaunch: the layout restores.
- Split a pane running a live process (e.g. `sleep 300`): the original pane
  keeps running after the split, and closing one side never kills the other.
  Panes also survive tab/workspace switches untouched.
- Workspaces can be closed from the sidebar; closing the last one resets fresh.
- Close the window: the app hides to the tray (with a "keeps running"
  notification) and panes keep running. Left-click the tray icon to show it
  again; Quit lives in the tray menu.
- Run an agent CLI (e.g. `claude`, `codex`, `opencode`) in one pane and switch
  to another tab: the pane header, tab, and sidebar show a working badge. When
  the agent exits while you're elsewhere, you get an OS notification and a
  review shortcut in the sidebar.
- The sidebar Agents section lists working agents (green), finished ones
  (check), unexpected stops needing review (amber), and idle known agents
  (hollow), grouped per workspace; clicking a row jumps to its pane.
- In Settings, switch Agent-finished delivery to in-app toast and finish an
  agent in another tab: a toast appears with a chime, and clicking it jumps
  to the pane. Muting that agent's CLI silences the chime but keeps the
  toast; the test-sound button previews the current chime.
- Right-click workspaces (Rename/Close), tabs (Rename/Close), and panes
  (Rename/Zoom/Close). Zoom fills the tab; hidden siblings keep running.
- Resize/move the window, quit, and relaunch: size and position restore.
- Keyboard shortcuts work with terminal focus: `Mod+T` new tab, `Mod+W` close
  pane, `Mod+N` new workspace, `Mod+D` / `Mod+Shift+D` split, `Mod+Enter`
  zoom, `Mod+[` / `Mod+]` switch tabs, `Mod+,` settings (`Mod` is Cmd on
  macOS, Ctrl elsewhere; the full list lives in Settings).
- `Mod+=` / `Mod+-` / `Mod+0` resize the terminal font; the size persists.
- Settings (sidebar footer, or `Mod+,`) holds launch-at-login, theme, font
  size, notification delivery, sounds and per-agent muting, the shortcut
  reference, and version info.

## Roadmap

- [x] Phase 0: scaffold + verified dev/build loop
- [x] Phase 1: PTY vertical slice (`portable-pty` + xterm.js pane)
- [x] Phase 2: multiplexer model, layout persistence, tray behavior
- [x] Phase 3: agent awareness v1 (process detection + badges)
- [ ] Phase 4 (later): blocked/done heuristics, split daemon + CLI, SSH remotes

## Contributing

Small, focused PRs merge fastest — see [CONTRIBUTING.md](CONTRIBUTING.md).
By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Don't open public issues for vulnerabilities — see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © 2026 Oliver Martinez
