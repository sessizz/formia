# Localization checks

Run the unit tests with Node.js 22 or newer:

```sh
node --test tests/*.test.mjs
```

The browser test is optional and is skipped unless `FORMIA_PLAYWRIGHT` points to the `index.mjs` file of a separately installed `playwright-core` package. Set `FORMIA_BROWSER` to a Chromium-compatible browser executable. Keep test dependencies outside this repository; the application has no new runtime dependencies.

The browser test starts a temporary loopback HTTP server, mocks the Tauri bridge, and stores its settings in isolated browser contexts. It does not launch Formia or read/write the user's settings or model files. It checks tr-TR/en-US/de-DE first-run behavior, live switching, settings persistence, and preview/scan continuity. Temporary screenshots are saved in the OS temporary directory. The optional dependency is needed only for this browser check.
