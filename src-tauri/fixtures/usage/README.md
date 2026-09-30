# Usage fixture provenance

Response shapes for the `usage` providers. Values are synthetic and harmless;
no fixture contains a real token, account id, or email. Key names come from
the sources below; no CLI tasks were run and no user configuration was
changed to collect them.

Inspected on 2026-09-30:

| Fixture | Provenance |
| --- | --- |
| `codex_wham.json` | Live `GET chatgpt.com/backend-api/wham/usage` key structure, captured via a key-names-only probe (values replaced). `secondary_window` populated here to cover the weekly lane; the live account returned `null`. |
| `codex_wham_no_weekly.json` | Same capture: the exact live case with `secondary_window: null`. |
| `codex_auth_shape.json` | Live `~/.codex/auth.json` top-level and `tokens` key names (values redacted). Codex 0.158.0. |
| `claude_oauth.json` | Window keys (`five_hour`, `seven_day`, `seven_day_sonnet`) per CodexBar's Claude provider doc (OAuth mapping section). Subfields (`utilization`, `resets_at`) are best-effort probe candidates — NOT live-captured (no OAuth session on this machine). Re-verify against a live OAuth session. |
| `claude_creds_shape.json` | Key names from literals in the installed Claude Code 2.1.284 executable (`claudeAiOauth` object with `accessToken`, `refreshToken`, `expiresAt`, …). Values redacted. |
