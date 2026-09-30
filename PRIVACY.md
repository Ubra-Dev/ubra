# Privacy and network behavior

This document describes local storage, provider usage, and the optional PostHog
integration. Analytics details apply only to builds containing that integration;
release notes should disclose the build-time settings used for each release.
The checked-in CI and release workflows explicitly leave both PostHog variables
empty, so official builds from those workflows do not initialize PostHog.

## Local terminal data

Ubra runs shells and agent CLIs on your computer. Settings, saved layouts, terminal
recovery data, and daemon state are stored locally. Commands and agents launched
inside terminals have their own network behavior and privacy policies. Local
terminal execution does not mean the entire app is offline.

## Agent usage requests

The Usage feature reads existing local Codex and Claude Code credentials to query
their providers' authenticated usage endpoints. Requests go to
`chatgpt.com/backend-api/wham/usage` and `api.anthropic.com/api/oauth/usage` from the
Rust backend. Credentials are sensitive; do not include them in bug reports.

## PostHog analytics and diagnostics

The current frontend initializes PostHog when both `PUBLIC_POSTHOG_PROJECT_TOKEN`
and `PUBLIC_POSTHOG_HOST` are configured at build time. It enables exception capture
and lifecycle logs and explicitly reports app boot and onboarding completion or
skipping. The SDK's default automatic event capture is not explicitly disabled.
Remote project configuration may also affect SDK behavior, including session
recording. Do not assume that collection is limited to the explicit events.

Events and diagnostics may contain SDK-generated identifiers, device/browser
metadata, interaction details, and error information. Error messages or captured
interface content can include sensitive information. The current code has no
Ubra-specific consent control or comprehensive data-scrubbing guarantee.

For a production build without this initialization, leave both PostHog variables
empty. The current development build throws when either value is missing; changing
that behavior is separate work. Distributors must document their analytics host,
collection configuration, retention/access policy, and any consent mechanism
before distributing an analytics-enabled build.

This policy documents existing behavior; it does not introduce analytics consent.
Privacy questions can be sent to `nemoryoliver@gmail.com`.
