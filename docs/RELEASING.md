# Releasing Ubra

## Before tagging

1. Merge through a pull request with all three `build (macos-latest)`,
   `build (ubuntu-latest)`, and `build (windows-latest)` CI checks passing.
2. Check version consistency in npm, Cargo, and Tauri manifests. Review third-party
   dependencies and notices, privacy configuration, and user-facing release notes.
3. Configure signing credentials below. The release build fails if macOS or Windows
   credentials are missing. Ordinary PR/CI builds do not require signing secrets.
4. Tag the reviewed commit `v<version>`. The release workflow reruns CI before
   building installers. Do not tag unrelated working-tree changes.

## Signing credentials

Store these as repository Actions secrets; never paste their contents into issues,
logs, or source files. The `release` environment still controls publication.

| Platform | Required secrets |
| --- | --- |
| macOS | `APPLE_CERTIFICATE` (base64 Developer ID Application `.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `KEYCHAIN_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID` |
| Windows | `WINDOWS_CERTIFICATE` (base64 exportable code-signing `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` |

macOS CI imports the certificate into a temporary keychain, passes signing and
notarization credentials to Tauri, then verifies the app with `codesign`, `stapler`,
and `spctl`. Windows CI imports the PFX into the runner's user certificate store,
configures SHA-256 timestamped signing, and verifies the executable, staged sidecars,
and installers with Authenticode. Certificates are removed after the build.

This Windows setup assumes an exportable PFX. If your certificate uses hardware or
remote signing, implement that provider's signing integration before tagging; do
not disable signature verification. Signing is not verified until a real platform
release job passes with valid credentials.

References: [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/),
[Tauri Windows signing](https://v2.tauri.app/distribute/sign/windows/).

## Native release verification

Download the `bundle-macOS`, `bundle-Linux`, and `bundle-Windows` artifacts. Test
each on its native OS and record the commit, installer, SHA-256, platform, and
results in the release review. Check launch, PTY interaction, close/quit/reattach,
tray behavior, persistence/recovery, and the installed `legal/` notices directory.
Verify signing/notarization on installed macOS and Windows bundles as well.

Set the matching repository Actions variable to the full tagged commit SHA only
after that platform passes:

- `NATIVE_SMOKE_SHA_MACOS`
- `NATIVE_SMOKE_SHA_LINUX`
- `NATIVE_SMOKE_SHA_WINDOWS`

Leave failed or unavailable platforms unset. The existing publication gate checks
all three variables and remains mandatory. Rerun publication only when all evidence
is recorded; do not substitute CI success for native verification.

## Published assets

The publication job creates a flat set of installer/download files and `SHA256SUMS`.
It also publishes `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `dependency-inventory.json`,
and `THIRD_PARTY_SOURCES.tar.gz` with corresponding source for locked MPL components.
Preserve these legal assets when redistributing the binaries. If you modify an MPL
component, ensure the archive includes the modified corresponding source.

Use `shasum -a 256 -c SHA256SUMS` on macOS or `sha256sum -c SHA256SUMS` on Linux after
downloading all assets into one directory. On Windows, compare `Get-FileHash -Algorithm
SHA256 <file>` with the manifest. Checksums detect file changes; they do not replace
signature verification.

## Repository settings

Main requires pull requests, up-to-date passing platform checks, and resolved
conversations; force pushes and branch deletion are disabled. These requirements
also apply to administrators. With one maintainer, external approval is not required;
add a required reviewer when another maintainer joins.

Discussions, private vulnerability reporting, Dependabot security updates, secret
scanning, and secret push protection are enabled. CI is enabled. Settings live on
GitHub and should be rechecked after transfers or policy changes.

## Funding launch

Complete the enrollment and live-link checklist in [SPONSORSHIP.md](../SPONSORSHIP.md).
Identity verification, tax forms, payout details, signing certificates, and real
native release verification remain maintainer tasks; configuration files do not
establish that these steps have passed.
