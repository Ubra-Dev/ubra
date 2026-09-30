# Bundled sidecars (generated)

`npm run tauri:build` builds release bundles with the daemon + CLI sidecars:
it merges `tauri.release.conf.json` (declaring `externalBin`), whose
`beforeBuildCommand` stages the bins here with the host target-triple
suffix Tauri expects. The bundler strips the suffix and places both next to
the app binary. Plain `cargo build` / `tauri dev` / `tauri build` stay
sidecar-free so tests and dev never depend on staged files; in dev the bins
land next to the app binary automatically. Never commit staged binaries;
this directory's contents are ignored.
