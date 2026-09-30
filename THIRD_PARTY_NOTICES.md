# Third-party notices

Ubra's original code is MIT-licensed; third-party components retain their own
copyrights and licenses. Ubra's license does not replace those terms.

## Icons

`src/lib/Icon.svelte` includes Feather icon geometry, including settings, layers,
and activity. Feather is copyright (c) 2013–2023 Cole Bemis and is MIT-licensed.
The full notice is preserved in `third_party/licenses/Feather-MIT.txt`.

## Dependencies and fonts

The frontend includes Svelte, xterm.js, Tauri APIs/plugins, PostHog, and Fontsource
fonts. The backend includes Tauri, portable-pty, terminal emulation, audio, and
their dependencies. Keep notices for transitive components as well as direct ones.
Font licenses are independent of Ubra's MIT license.

The published `@posthog/browser-common@0.9.0` package declares MIT but supplies no
standalone license or copyright notice. Its supplemental notice preserves that
versioned declaration and the standard MIT terms without inventing a copyright
holder. Review this entry with upstream when updating the package.

Run `npm run licenses` after installing npm dependencies and fetching locked Rust
dependencies. It produces an inventory and full available license/notice texts
under `.tooling/legal/`. The Tauri build includes those notices as resources.
Missing license texts fail generation; supplemental upstream texts are recorded
with version-specific provenance in `third_party/license-overrides.json`.

The generator also prepares corresponding Rust source for MPL-2.0 components.
Release publication includes that source archive and license notices alongside
the installers. Multiple-license expressions retain their upstream terms; a
component's restrictive alternative need not be selected when a permissive
alternative applies. Changes to components with source-disclosure obligations
must preserve those obligations.

This inventory conservatively includes locked Rust dependencies for all platforms,
including build-only components. It is not a certification of ownership or a
complete audit of native system libraries, generated assets, or redistributed
platform runtimes. Review new dependencies and attribution before releases.

## Design references

Herdr's session-state and detached-launch design informed Ubra's terminal
persistence architecture, as linked in README. This acknowledgement does not
claim affiliation or permission to use Herdr branding. Preserve any applicable
upstream notices if code is imported in future.
