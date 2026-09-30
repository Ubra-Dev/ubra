# Contributing to Ubra

Thanks for picking this up. Small, focused PRs merge fastest.

## Ground rules

- **Correctness first, then maintainability.** Prefer boring solutions; delete
  weightless code; no drive-by refactors or new abstractions "while you're at it".
- **Clean cutover.** Migrate every caller; no shims, aliases, or deprecated paths.
- **Second conventions are prohibited.** Reuse the existing pattern in the file
  you're touching.

## Workflow

1. Fork, branch from `main`, keep the diff tight.
2. Before modifying an exported symbol, check its callsites so none are missed.
3. Verify before you push (same checks CI runs):
   ```bash
   npm run licenses    # prepare legal resources before Rust builds/tests
   npm run test:unit   # frontend unit tests
   node --test tests/release-tooling.test.mjs
   npm run check       # svelte-check
   npm run build       # static frontend build
   npm audit           # frontend dependency advisories
   cargo test --manifest-path src-tauri/Cargo.toml
   cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
   cargo fmt --manifest-path src-tauri/Cargo.toml --check
   node scripts/smoke-daemon.mjs
   npm run tauri build # native bundle and third-party notices
   ```
4. Tests earn their place only where a plausible bug would fail them — behavior,
   boundaries, invariants, real errors. No implementation-assertion or padding tests.

## What to work on

- Open issues labeled `good first issue` are scoped and reviewed quickly.
- Bug reports should include: what you did, what you expected, what happened,
  plus your OS and the app version or commit hash.

## Commit messages

Conventional style: `feat:`, `fix:`, `docs:`, `chore:` … — one logical change
per commit, present tense ("add X", not "added X").

## Security

Don't open public issues for vulnerabilities — see [SECURITY.md](SECURITY.md).

## Contribution licensing and attribution

By submitting a contribution for inclusion in Ubra, you agree to license your
contribution under the repository's MIT license. You retain copyright in your
work; no copyright assignment or contributor license agreement is required.
Submit only work you own or have permission to contribute under compatible terms.

Identify copied or adapted code and assets in your pull request, including the
upstream URL, version/commit, and license. Preserve required copyright and license
notices. Third-party files keep their applicable licenses; do not replace them
with Ubra's MIT notice. Update [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
run `npm run licenses` when dependencies or imported assets change.

Discuss substantial changes before coding. See [GOVERNANCE.md](GOVERNANCE.md) for
decisions and releases and [SUPPORT.md](SUPPORT.md) for usage questions.
