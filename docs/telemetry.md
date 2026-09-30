# Telemetry

Ubra ships opt-in PostHog telemetry in the published Mac app: anonymous
product analytics plus crash reporting. Community builds omit the keys and
the entire pipeline compiles to a no-op.

Two lanes share one PostHog project and one consent record:

- **Frontend lane** (`posthog-js`, initialized in `src/hooks.client.ts`):
  lifecycle log records (`src/lib/posthogLogs.ts`) and JS exception
  autocapture.
- **Backend lane** (`posthog-rs`, `src-tauri/src/telemetry.rs`): consent
  storage, Rust panic capture, allowlisted agent events, and the
  feature-flag snapshot.

## Consent

- Opt-in, default off. Consent lives in `{data_dir}/telemetry.json`,
  owned by Rust so the panic hook and client init stay gated before the
  frontend loads. Absent or unreadable file = declined.
- Users opt in during onboarding (`FirstRun.svelte`) or anytime in
  Settings → App → Telemetry. The Settings section hides entirely in
  builds without a key.
- Boot (`src/routes/+page.svelte`) and every toggle sync the JS client
  via `opt_in/out_capturing()` (`src/lib/telemetrySync.ts`).
- Revoking stops new capture immediately and restores the previous panic
  hook. At most one batch queued before revocation may still deliver.
- Consent state: `telemetry_status` → `{ supported, consented, active }`.

## Event catalog

| Event | Source | Properties | Identity |
|---|---|---|---|
| `application_booted` | posthog-js log | `component` | identified |
| `onboarding_completed` / `onboarding_skipped` | posthog-js log | `component` | identified |
| JS `$exception` | posthog-js autocapture | SDK standard | identified |
| `agent_cli_started` | Rust `telemetry_capture` | `cli` (known stem only) | anonymous |
| `agent_cli_ended` | Rust `telemetry_capture` | `cli`, `duration` bucket, `outcome` | anonymous |
| Rust `$exception` (panics) | Rust panic hook | message, file:line, stack | stored id or personless |

Durations are coarse buckets (`under_minute`, `under_5m`, `under_30m`,
`over_30m`) measured from first observation; they reset on app restart.
Outcomes: `completed`, `stopped`, `closed`.

## Never collected

Commands, arguments, code, file contents, file paths, terminal output.
Consequently these PostHog features are banned in this app: session
replay, heatmaps, autocapture, in-webview surveys. PostHog's LLM
analytics doesn't apply (the app makes no LLM API calls), and
experiments/CDP/warehouse are deferred until there is scale or revenue.

## Scrubbing (`telemetry::scrub`, unit-tested)

- The home directory becomes `~`; panic locations keep only the file name.
- `token=`/`secret=`/`password=`-style assignments are masked (quoted and
  bare, case-insensitive); `phc_*` keys are masked.
- Output is capped at 2000 chars.

## Feature flags

One flag exists: `agent-events-enabled`, a kill switch for the Rust agent
event pipeline. Evaluation fails open (feature on) when telemetry is off,
offline, or unevaluated. The snapshot refreshes on startup and consent
grant (background thread, never blocking), persists to
`telemetry-flags.json` for offline runs, and costs one
`$feature_flag_called` exposure event per flag per refresh. Frontend code
should call `posthog.getFeatureFlag` directly; no wrapper is provided.

## Build flavors

- Frontend lane: `PUBLIC_POSTHOG_PROJECT_TOKEN` / `PUBLIC_POSTHOG_HOST`
  (see `.env.example`). Missing in production = silent no-op.
- Backend lane: `POSTHOG_KEY` / `POSTHOG_HOST` in the `tauri build`
  environment. Missing = `supported() == false`, all commands no-op.
- Release packaging must set both pairs; nothing telemetry-related is
  committed to the repo.

## Verifying

Declined-boot network audit (the highest-risk check): launch with consent
declined and confirm zero requests to `*.posthog.com` (devtools network
or a proxy). Note: until the companion edit below lands, the JS client
initializes before the boot sync runs, leaving a small race window.

## Ops runbook (maintainer, PostHog UI)

- **Crash-spike alert:** alert on `$exception` volume surge after releases.
- **Dashboards:** funnel `application_booted` → `onboarding_completed`
  → first `agent_cli_started`; agent mix by `cli`; end outcomes.
- **Annotations:** mark each release.
- **Follow-ups:** release debug-symbol uploads (release builds are
  stripped, so production Rust frames need symbols); Sentry if
  abort-profile panics prove lossy.

## Companion edit (other lane)

One line in `src/hooks.client.ts` closes the boot race — initialize the
client opted out so nothing can emit before the consent sync:

```ts
posthog.init(PUBLIC_POSTHOG_PROJECT_TOKEN, {
  opt_out_capturing_by_default: true,
  // ...existing options
});
```

Also reconcile the dev-mode `throw` on missing env with the no-op
philosophy (warn instead), so keyless checkouts keep running `tauri dev`.
