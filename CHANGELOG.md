# Changelog

All notable changes to Ubra are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

- Terminal panes reattach to their surviving backend session across frontend
  restarts (hot reload) by stable pane key instead of spawning blank
  replacements; sessions whose panes no longer exist are reaped on load.
- Pane snapshots now include scrollback history, so reattached terminals
  keep their history, not just the visible screen.
- Panes remember their last agent CLI and rerun it when they spawn fresh
  (relaunch or explicit restart); the restart button names the agent.
- Terminal dispose no longer kills its PTY while the layout is unloaded,
  fixing agent loss during hot-reload windows.

## [0.1.0] — Initial development snapshot

Early desktop agent runtime: workspace/tab/pane multiplexer, layout
persistence and recovery, tray behavior, agent CLI awareness with badges
and notifications, Explorer and Source Control sidebar, experimental
headless daemon + CLI, and signed auto-updating releases.
