# Releasing Ubra

Maintainer guide for cutting a signed, smoke-tested release. End users:
see the [README](../README.md) and [Releases](https://github.com/Ubra-Dev/ubra/releases).

## Release gates and native smoke

Pushing `v*` tags invokes the full CI matrix first. Only after every platform
passes does the release workflow build and stage its bundles. GitHub release
publication is then **blocked by default** until all three repository Actions
variables below contain the tagged commit's full SHA:

- `NATIVE_SMOKE_SHA_MACOS`
- `NATIVE_SMOKE_SHA_LINUX`
- `NATIVE_SMOKE_SHA_WINDOWS`

Download the `bundle-macOS-arm64`, `bundle-macOS-intel`, `bundle-Linux`, and
`bundle-Windows` artifacts from that release run and smoke-test each on its
native OS. Both macOS artifacts share the `NATIVE_SMOKE_SHA_MACOS` variable:
smoke each architecture where hardware allows and record which builds were
covered. Record the commit, OS, bundle, checks and failures in the release
review before setting that platform's variable. A failed or unavailable
platform remains unverified: leave its variable unset and do not publish.
After all evidence is recorded, rerun the failed publish job. The `release`
environment can additionally require maintainer approval; the SHA checks fail
closed even if environment reviewers are not configured.

Neither automated bundle creation nor this documentation establishes a successful
native smoke run.

## Release signing and updates (one-time maintainer setup)

macOS releases are signed with a Developer ID identity, notarized, and stapled
automatically by the Tauri bundler when the workflow secrets below exist. The
release refuses to build for macOS without `APPLE_SIGNING_IDENTITY`, so an
unsigned DMG can never ship silently. Set these repository secrets once:

- `APPLE_CERTIFICATE` — base64 of the exported Developer ID Application `.p12`
- `APPLE_CERTIFICATE_PASSWORD` — the `.p12` export password
- `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Name (TEAMID)`
- `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID`
- `TAURI_SIGNING_PRIVATE_KEY` (+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) — see below

Generate the updater keypair once with `npm run tauri signer generate -w
~/.tauri/ubra.key`, paste the **public** key into `tauri.conf.json` →
`plugins.updater.pubkey` (replacing the placeholder), and store the private key
as `TAURI_SIGNING_PRIVATE_KEY`. Never commit the private key; losing it breaks
the update chain for installed apps, since each release's bundles are verified
against the embedded public key.

The workflow builds separate arm64 and Intel macOS targets (one universal
build is not possible: the Tauri CLI only lipo-merges the main binary, while
Ubra must bundle the sibling `ubra-daemon` and `ubra-cli` binaries), fails
when the tag does not match `tauri.conf.json`'s version, and publishes
`latest.json` alongside the bundles so installed apps can update from
Settings → Updates.

Native macOS smoke must additionally verify the signature chain on the staged
DMG: `codesign -dv --verbose=4 Ubra.app` shows the Developer ID identity,
`spctl -a -vv Ubra.app` accepts it, `stapler validate` passes on the app and
the DMG, and a quarantine-flagged copy opens without a Gatekeeper block. After
the first signed release, updater smoke is: install release N, publish N+1,
confirm Settings → Updates offers, installs, and relaunches into N+1.

Local builds without the signing keys must pass `--no-sign`
(`npm run tauri build -- --no-sign`); the bundler fails closed when a public
updater key is configured but `TAURI_SIGNING_PRIVATE_KEY` is absent.
