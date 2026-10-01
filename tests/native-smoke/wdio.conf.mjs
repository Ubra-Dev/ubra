export const config = {
  runner: "local",
  specs: ["./release.spec.mjs"],
  maxInstances: 1,
  logLevel: "info",
  framework: "mocha",
  reporters: ["spec"],
  services: [["@wdio/tauri-service", {
    appBinaryPath: process.env.APP_BINARY,
    driverProvider: process.platform === "win32" ? "external" : "embedded",
    autoInstallTauriDriver: process.platform === "win32",
    autoDownloadEdgeDriver: process.platform === "win32",
    embeddedPort: 4445,
    appArgs: [],
  }]],
  capabilities: [{
    browserName: "tauri",
    "tauri:options": { application: process.env.APP_BINARY },
  }],
  mochaOpts: { timeout: 120_000 },
  waitforTimeout: 30_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 2,
};
