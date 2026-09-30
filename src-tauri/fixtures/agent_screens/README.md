# Agent screen rules and fixture provenance

These files are **synthetic renderer-derived marker cases**, not captured user
sessions. They omit unrelated layout and replace example commands, durations,
and answers with harmless values. They prove matching behavior for supported
markers; they do not claim end-to-end validation of every installed UI mode.
No CLI tasks were run, and no user configuration was changed to collect them.

Inspected on 2026-09-30:

| CLI | Version | Marker provenance | Supported evidence |
| --- | --- | --- | --- |
| Codex | 0.158.0 | Installed executable literals; upstream Rust `status_indicator_widget.rs`, `bottom_pane/approval_overlay.rs`, and `bottom_pane/chat_composer.rs` | Busy interrupt hint, command/edit/permission approvals, ready composer with shortcut footer |
| Claude | 2.1.284 | JavaScript embedded in installed executable: confirmation renderer `_q` (question, selected options, escape cancel), `zMt` (interrupt footer), composer shortcut footer | Standard proceed approval, busy interrupt footer, ready composer with shortcut footer |
| Gemini | 0.57.0 | Installed npm bundle `interactiveCli-5O6FZS57.js`: `LoadingIndicator`, `ToolConfirmationMessage`, `InputPrompt`, `AskUserDialog`, `TextQuestionView`, `ReviewScreen`; `DialogFooter` in dependency chunk | Responding indicator, execution/edit/info approvals, empty composer, text question and answer review |
| OpenCode | 1.18.30 | Primary tagged `packages/tui/src/component/prompt/index.tsx` renderer; upstream `packages/tui/src/routes/session/permission.tsx` renderer (approval source not version pinned) | Busy interrupt footer, empty composer with commands footer, permission option group with selection/confirmation footer |
| MiMoCode | 0.1.15 | JavaScript embedded in installed executable (Bun bundle): permission renderer (titled dialog, once/always/reject options), session footer (spinner row with `esc interrupt` / `esc again to interrupt`), prompt hints (idle-only `@`/`$`/`/` labels) | Busy interrupt footer, empty composer with idle hints, permission option group with selection/confirmation footer, text question |
| Antigravity (`agy`) | 1.2.13 | Go binary literals: generating footer, conversation-scoped permission option labels | Busy generating footer, permission allow/deny options (partial: no composer rule, so no working→idle completion yet) |
| Cursor Agent CLI | 2026.09.28-64d2043 | Bundled JS (`6949.index.js`): per-tool approval questions and single-key option hints | Permission option hints incl. delete confirm (partial: no busy/composer rules yet) |

Codex primary sources:

- https://github.com/openai/codex/blob/main/codex-rs/tui/src/status_indicator_widget.rs
- https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/approval_overlay.rs
- https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/chat_composer.rs

The main-branch source is supplementary evidence, not an assertion that all
upstream markers exist in the inspected binary. Approval strings and shortcut
text were also checked against that executable. OpenCode primary sources:

- https://github.com/anomalyco/opencode/blob/v1.18.30/packages/tui/src/component/prompt/index.tsx
- https://github.com/anomalyco/opencode/blob/dev/packages/tui/src/routes/session/permission.tsx

The OpenCode approval case is backed by upstream source, not verified against a
real 1.18.30 terminal; version differences can therefore yield Unknown.

Custom keybindings, brief modes, alternate composer modes (MiMoCode shell
mode uses a different placeholder), other approval/question variants, narrow
truncation, unsupported versions, and CLIs without a validated profile can
yield Unknown. A changed screen never supplies evidence by itself.

Bundled rules use a small bottom window, supporting markers, and a footer anchor
where available. Trailing empty VT rows are ignored. Case and whitespace at word
boundaries are normalized. A main composer below an old Codex or Claude approval
menu invalidates that menu; Gemini's empty composer invalidates old approvals.
Busy indicators are also bound to composer ordering: Codex and Gemini place
their indicator above their own composer, while Claude and OpenCode place the
interrupt footer below it. A newer composer invalidates that historical busy
evidence even when both screens still fit in the bottom matching window.
Ambiguous historical markers may yield Unknown rather than Working.
Unmatched or partial markers yield no evidence.

## Overrides

A valid `<cli>.toml` in the existing `agent-detection` directory replaces the
entire CLI profile, including its bundled working/idle rules. Legacy files
continue to use case-insensitive AND matching over the bottom forty lines:

```toml
[[blocked]]
id = "approval"
contains = ["allow this command", "❯"]
```

New sections and stricter matching are optional:

```toml
[[working]]
id = "active-task-footer"
contains = ["running"]
window_lines = 6
line_prefixes = ["status:"]
tail_contains = ["cancel"]
not_contains = ["ready"]

[[idle]]
id = "ready-prompt"
contains = ["ready", ">"]
window_lines = 4
```

All `contains` markers must match. Any `not_contains` match rejects the rule.
At least one `line_prefixes` prefix must match a normalized line when provided.
All `tail_contains` markers must be in the last three used rows. `window_lines`
must be 1–40. Invalid files are ignored with a warning; empty/incomplete rules
are skipped. Blocked takes precedence over Working, then Idle.

Before changing profiles, add sanitized real captures with the CLI version and
rendering mode recorded here. Include ready, responding, approval/question,
completion, cancellation, wrapping, and stale approval history examples. Keep
real capture provenance distinct from these source-derived regression cases.
