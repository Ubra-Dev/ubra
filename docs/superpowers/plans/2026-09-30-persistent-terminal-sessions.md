# Keep Ubra terminals alive across quitting and recover them after reboot

## Summary

Move terminal ownership into Ubra’s existing background daemon. Quitting Ubra will disconnect the desktop UI; reopening it will attach to the same processes and restore their terminal screens.

After a daemon or computer restart, restore saved panes and directories, replay recent output, and resume agent conversations when their exact session references are available. This follows [Herdr’s live persistence and restart model](https://github.com/herdrdev/herdr/blob/master/docs/preview/website/src/content/docs/session-state.mdx).

Chosen defaults: screen history enabled, automatic recovery for all detected agents with a usable resume integration, and a restored shell with an explanatory notice when conversation recovery is unavailable.

## Runtime and desktop connection

- Make the daemon the sole owner of PTYs, terminal emulation, agent status, and recovery records. Replace the GUI’s in-process `PtyManager` with an authenticated Rust connection that proxies commands and bridges daemon events into the existing Tauri events.
- Share one launch/connect implementation between desktop and CLI. Detach the daemon from its launcher using Unix `setsid`, detached Windows process creation, and the macOS user service context needed to escape the launching terminal’s lifetime. Use [Herdr’s platform launch implementation](https://github.com/herdrdev/herdr/blob/master/src/platform/mod.rs) as the reference.
- Bundle matching daemon and CLI executables on macOS, Linux, and Windows; build them automatically for development as well.
- Give each daemon lifetime a unique runtime epoch. Associate each saved pane’s existing string ID with its runtime session, independently of numeric PTY IDs.
- Extend the protocol with attach-or-create by stable pane ID, terminal snapshots, explicit pane closure/restart, recovery activation, and agent session reporting. Return whether attachment reused, created, recovered, or found an exited session.
- Include the runtime epoch and session incarnation in terminal operations and events so stale requests cannot affect replacement processes. Reconnect automatically after transport loss; restore a snapshot before continuing live output.
- Preserve the current frame and queue limits. Transfer large screen/history snapshots in bounded pages from an immutable snapshot with a sequence watermark; reuse the frontend’s snapshot-and-output ordering logic.
- Keep terminal query responses owned by the daemon when xterm is attached, preventing duplicate responses from the daemon and renderer.
- Bump the protocol version. Report an incompatible running daemon with restart instructions; never stop existing sessions automatically during an app upgrade.

## Persistence and lifecycle

- Keep the existing layout format and pane IDs. Route layout commits through the daemon so layout removal also closes and removes the associated runtime records, including terminals in hidden tabs.
- Store versioned recovery checkpoints in the persistent app data directory, separate from temporary socket/authentication files. Honor `UBRA_DATA_DIR` consistently in GUI, CLI, and daemon.
- Save each pane’s launch configuration, last confirmed directory, dimensions, recent terminal history, exit/recovery state, and exact agent resume reference. Link recovery records to the committed layout so stale output cannot attach to a different pane.
- Checkpoint structural changes and session-reference updates immediately. Save dirty terminal history at most once per second and flush on orderly daemon shutdown. Use atomic replacement and private file permissions; preserve unreadable or unsupported saved documents through the existing recovery workflow.
- Retain scrollback according to the existing setting, capped at 4 MiB of recent history per pane. Extend backend snapshots to include scrollback, primary/alternate screens, cursor, terminal modes, and unfinished escape sequences.
- On normal app quit—including Settings, tray, keyboard quit, or closing without a tray—flush pending layout changes and disconnect. Leave all terminal processes running.
- Preserve close-to-tray behavior where currently available. Update its messages and Settings copy to explain that quitting also leaves terminals running.
- Add **Stop all terminals and quit** as a separate confirmed action. It terminates sessions and removes their automatic recovery records while retaining the layout.
- Explicit pane/tab/workspace closure terminates affected processes and removes recovery data. Natural process exit retains the final screen and exited state; reopening the GUI does not silently rerun the command.
- After a daemon restart, load recovery records without immediately running agents. When Ubra first attaches and supplies terminal context, recover eligible panes across all workspaces and tabs, including hidden ones.
- An unavailable directory or executable leaves the pane visible with its saved data and a retry action. Do not silently move it elsewhere.

## Agent conversation recovery

- Introduce one adapter registry covering every family and alias in the existing detected-agent table. Add missing official executable aliases such as `kiro-cli`.
- Implement Ubra session-reporting hooks/plugins for Herdr’s native restore families and the additional detected agents with resume facilities: Gemini, Amp, Aider, Goose, Crush, Kiro, Cline, Muse, Maki, and MiMoCode.
- Use hooks or plugins to report the exact main conversation ID/path and resume arguments. Support explicit agent-reported resume commands through the same protocol. Do not infer identity from the newest session file or project directory.
- Pass pane identity, runtime/session incarnation, daemon location, and CLI path into spawned processes. Reject stale reports and exclude child-agent conversations from the pane’s main recovery reference.
- Provide idempotent integration installation/status/removal in Settings. Merge Ubra-owned entries while preserving existing user and Herdr integrations; scope reporting to Ubra panes and keep hook output compatible with each agent.
- Preserve model and relevant launch options. Resume through each agent’s supported command, without adding prompts, submitting approvals, or introducing stronger permission flags.
- Deduplicate recovery by agent, exact conversation reference, and directory. Resume each eligible conversation once per recovery cycle.
- Clear recovery eligibility when the user deliberately exits the agent or closes its pane. Preserve it through abrupt runtime loss.
- For unsupported versions, missing integrations, unavailable sessions, or ambiguous references, restore the shell and saved output with **Conversation could not be resumed** and a retry action. This is the fallback you selected.
- Add settings for automatic agent recovery and saving screen history, both enabled by default. Disabling history saving removes Ubra’s persisted screen history.

## Validation and delivery

- **Live persistence:** run a long-lived process, quit Ubra, reopen it, and verify the same process/session identity, screen, and continued input. Repeat after closing the terminal application that launched Ubra.
- **Recovery:** restart the isolated daemon and verify layout, directories, history, and exact conversation recovery across visible and hidden panes. Confirm arbitrary programs return as shells.
- **Identity and races:** cover concurrent attachment, reconnect during output, large snapshots, stale events after restart, and two agents in the same directory resuming different conversations.
- **Closure:** verify hidden-tab closure terminates processes, closed panes never return, exited commands stay exited, and Stop all terminals and quit prevents automatic agent recovery.
- **Adapters:** test report capture, session switching, child-session exclusion, resume argument construction, existing-hook coexistence, missing-session fallback, and integration removal for every registered family.
- **Durability:** test interrupted checkpoint writes, corrupt/future documents, unavailable directories, history limits, disabled-history cleanup, and isolated data directories.
- Run frontend checks/tests and Rust tests, lint, and formatting checks. Add installed-bundle launch/quit/reattach smoke coverage to the existing macOS/Linux/Windows CI matrix.
- Update user documentation with quit semantics, recovery settings, supported integration requirements, and recovery limitations. Preserve Herdr attribution and Apache license notices wherever its integration code is adapted.

Processes and shell memory survive GUI quit only while the daemon remains alive. After reboot, recovery starts fresh shells or native agent resume commands; it cannot revive arbitrary processes or guarantee continuation of an interrupted agent turn.
