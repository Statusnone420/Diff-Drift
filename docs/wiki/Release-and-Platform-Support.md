# Release and Platform Support

## Supported Platform

Windows 11 is the supported platform for the current public build.

Build the Windows NSIS installer:

```bash
npm run tauri -- build --bundles nsis
```

Expected output:

```text
src-tauri/target/release/bundle/nsis/Diff Drift_0.2.0_x64-setup.exe
```

## macOS Status

macOS is experimental and currently unsigned. Signing, notarization, updater setup, and release testing are not configured yet.

Do not imply macOS is production-ready until that work exists.

## Bundle Identifier

The configured identifier is:

```text
io.github.statusnone420.diffdrift
```

Treat it as frozen. Changing it later can break app identity, local data paths, update continuity, and macOS signing/notarization expectations.

## Versioning

The app version appears in:

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`

Keep them aligned before publishing a release.

## Release Checklist

- Run `npm run test:rust`.
- Run `npm run build`.
- Run `npm run test:e2e:web`.
- Run native E2E for native behavior changes.
- Build the installer.
- Smoke-test opening a real repo, dismissing/restoring a flag, marking reviewed, and exporting a report.
- Update README and docs only for behavior that actually exists.
