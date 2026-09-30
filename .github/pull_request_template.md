# What changed and why?

## Verification

- [ ] `npm run test:unit`
- [ ] `npm run check`
- [ ] `cargo test` (src-tauri)
- [ ] `cargo clippy --all-targets -- -D warnings` (if touching Rust)
- [ ] `cargo fmt --check` (if touching Rust)
- [ ] `npm run tauri build` (if touching build, bundling, or native paths)

## Checklist

- [ ] One logical change; every caller migrated (no shims/aliases)
- [ ] Follows the existing pattern in touched files
- [ ] Tests only where a plausible bug would fail them
- [ ] I have permission to contribute this work under MIT; imported components keep their original licenses
- [ ] Copied/adapted code or assets are identified with upstream source, version, and license (or none were imported)
- [ ] Required third-party notices are preserved; dependency/asset changes were checked with `npm run licenses`
