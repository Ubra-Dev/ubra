export const config = {
  runner: "local",
  specs: ["./tests/native-smoke/**/*.spec.mjs"],
  maxInstances: 1,
  logLevel: "info",
  framework: "mocha",
  reporters: ["spec"],
  services: [["@wdio/tauri-service", {
    appBinaryPath: process.env.APP_BINARY,
    driverProvider: "embedded",
    // The service passes this to the app; Rust registers the embedded driver
    // only when it is present.
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
