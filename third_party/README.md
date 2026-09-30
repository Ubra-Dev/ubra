# Supplemental upstream licenses

These files supplement license and copyright notices that published dependencies
omit. `license-overrides.json` maps an exact ecosystem/name/version to local texts
and their upstream provenance URLs. They retain their original licenses and must
not be relabeled as Ubra-owned MIT code.

Most Cargo supplements come from the Git commit recorded in the crate's
`.cargo_vcs_info.json`. The old winapi target packages lack that metadata; their
supplements come from the upstream 0.3 release line. Selectors points to the
official MPL 2.0 text. PostHog browser-common's supplement explains its published
MIT declaration and absent copyright notice rather than inventing a copyright
owner. Feather's MIT license is preserved separately for embedded icon geometry.

When upgrading a dependency:

1. Run `npm ci` and fetch locked Cargo dependencies.
2. Run `npm run licenses`. Missing notices stop generation.
3. If an upstream package omits its notices, retrieve the applicable license from
   that release's repository, preserve its copyright and all relevant notices,
   and record a version-specific mapping and source URL here.
4. Review changed license declarations and source-disclosure requirements. Update
   the generated inventory/source archive by rerunning generation.

This directory contains license texts and provenance, not a full vendored copy of
the dependencies. Build-generated notices and corresponding MPL source are under
`.tooling/legal/` and are packaged/published by the release workflow.
