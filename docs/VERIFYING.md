# Verifying Ubra manually

For release approval, use the **packaged app**, not only `tauri dev`. Launch it
with a fresh `UBRA_DATA_DIR` (for example `UBRA_DATA_DIR="$(mktemp -d)" <app-binary>`
on macOS/Linux, or set `$env:UBRA_DATA_DIR` to a new temporary directory before
launching on Windows). Preserve that scratch directory while testing relaunches.
Never point destructive probes at your normal layout or running daemon.

## Native platform checklist

1. Start the installed bundle, run a command that creates a marker file, and
   verify the file exists; input echo alone is not proof of command execution.
2. Exercise split/move/zoom while output continues and while a full-screen
   terminal program runs. Close one pane with a child process and verify its
   owned processes exit while a sibling continues.
3. Quit/relaunch to check persistence. In a separate scratch data directory,
   load corrupt and unsupported layouts: recovery must preserve the original
   until explicit reset, with retry/export errors visible.
4. Verify Settings, the notification chime, notification delivery, terminal
   clipboard, clickable links, and packaged webview console/CSP diagnostics.
5. Use keyboard-only onboarding, dialogs and context menus; focus must remain
   inside overlays and return on dismissal. Check navigation with many tabs,
   workspaces and agent rows.
6. Hide/show/quit with the tray; on Linux also exercise tray-unavailable close.
   Quitting must terminate owned sessions.
7. On Windows, also build the headless executables from the same tagged source
   (`cargo build --release --manifest-path src-tauri/Cargo.toml --bins`). They are
   separate from the GUI bundle. Run the daemon/CLI with a fresh `--state-dir`,
   execute a marker-file command through ConPTY, read output, close the pane and
   shut down. Check invalid commands return a nonzero exit code on every OS.
8. Open the right sidebar's Explorer on a scratch folder: browse, reveal, and
   copy paths. In a scratch git repo, verify Source Control status groups,
   diffs, staging, a commit, and push/pull against a local bare remote.

The development checklist below complements, but does not replace, these gates.

- Split panes from the hover toolbar, drag the dividers, add tabs/workspaces,
  then quit and relaunch: the layout restores.
- Split a pane running a live process (e.g. `sleep 300`): the original pane
  keeps running after the split, and closing one side never kills the other.
  Panes also survive tab/workspace switches untouched.
- Workspaces can be closed from the sidebar; closing the last one resets fresh.
- Close the window with a working tray: the app hides and panes keep running.
  Left-click the tray icon to show it again; Quit is available in Settings and
  the tray menu. Without a working tray, closing the window quits instead.
- Agent badges use explicit CLI screen evidence: Working means a recognized
  busy indicator, Blocked means an approval/question prompt or a suspended
  process, and Idle means a recognized ready prompt before an observed task.
  A task returning to its ready prompt becomes Done. Unknown means an agent
  is present but its activity cannot be established. Silence, typing, focus,
  and terminal redraws do not count as task activity or completion.
- The sidebar groups agents by workspace and working directory. Clicking a
  row reveals and focuses its pane. Focusing it in the foreground acknowledges
  unread completion/attention without changing runtime status. Hovering a row
  reveals an X that uses the existing pane-close confirmation.
- The right sidebar follows the active workspace folder: Explorer browses,
  opens, and reveals files; Source Control stages, commits, pushes, and pulls.
  Collapse state and commit drafts persist across view switches.
- Unexpected stops show Needs review; deliberate pane closes are silent.
  A directly launched agent's successful exit can confirm completion, but a
  parent shell's exit code does not establish the nested agent's success.
- Bundled screen profiles cover recognized Codex, Claude, Gemini, and OpenCode
  UI markers. Changed UI versions, custom keybindings, narrow truncation, and
  other CLIs can report Unknown. Rule provenance and supported markers are
  documented in [the screen fixture guide](../src-tauri/fixtures/agent_screens/README.md).
- Per-agent rules live in `<data-dir>/agent-detection/<cli>.toml`. Existing
  `[[blocked]]` sections retain their forty-line matching behavior. Optional
  `[[working]]` and `[[idle]]` sections support bounded footer matching; a file
  replaces that CLI's complete bundled profile.
- In Settings, switch Agent-finished delivery to in-app toast and finish an
  agent in another tab: a toast appears with a chime, and clicking it jumps
  to the pane. Muting that agent's CLI silences the chime but keeps the
  toast; the test-sound button previews the current chime.
- Right-click workspaces (Rename/Close), tabs (Rename/Close), and panes
  (Rename/Zoom/Move to new tab/Move to new workspace/Close). Zoom fills
  the tab; hidden siblings keep running. Moved panes keep their live
  terminals; the source tab keeps a fresh pane when emptied.
- `Mod+Alt+Arrow` moves focus between panes, `Mod+Alt+Shift+Arrow` swaps
  the focused pane with its neighbor, `Alt+Shift+Arrow` grows it, and
  `Mod+Alt+T` / `Mod+Alt+N` move it to a new tab / workspace.
- Resize/move the window, quit, and relaunch: size and position restore.
- Keyboard shortcuts work with terminal focus: `Mod+T` new tab, `Mod+W` close
  pane, `Mod+N` new workspace, `Mod+D` / `Mod+Shift+D` split, `Mod+Enter`
  zoom, `Mod+[` / `Mod+]` switch tabs, `Mod+,` settings (`Mod` is Cmd on
  macOS, Ctrl elsewhere; the full list lives in Settings).
- `Mod+=` / `Mod+-` / `Mod+0` resize the terminal font; the size persists.
- `Mod+F` (or pane menu → Find) opens terminal find with incremental
  highlight; Enter / Shift+Enter steps through matches, Esc closes.
- Terminal scrollback is configurable in Settings → Appearance; URLs
  printed in any pane are clickable and open in the browser.
- Settings → Appearance has a background opacity slider (10–100%) for
  terminal-style translucency; the sidebar, tabs, and dialogs stay solid.
- Settings (sidebar footer, or `Mod+,`) holds launch-at-login, theme, font
  size, the interface font picker (searchable bundled Google Fonts with
  previews), notification delivery, sounds and per-agent muting, the
  shortcut reference, and version info.
