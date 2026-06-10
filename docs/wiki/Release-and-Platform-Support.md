# Release and Platform Support

## Supported Platform

Windows 11 is the supported platform for the current public build.

Build the Windows installers:

```bash
npm run tauri -- build --bundles nsis,msi --ci
```

Expected output:

```text
src-tauri/target/release/bundle/nsis/Diff Drift_0.3.0_x64-setup.exe
src-tauri/target/release/bundle/msi/Diff Drift_0.3.0_x64_en-US.msi
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

Keep them aligned before publishing a release. Record user-visible changes in `CHANGELOG.md` under `[Unreleased]` as they land; the release moves them under the new version heading.

## Release Runbook

Releases are tag-driven. The [release workflow](../../.github/workflows/release.yml) builds the artifacts; a human verifies and publishes.

1. Verify the tree: `npm run test:rust`, `npm run build`, `npm run test:unit`, `npm run test:e2e:web`, `npm run eval:engine`. Run native E2E for native behavior changes.
2. Align the three version fields and move `CHANGELOG.md` entries from `[Unreleased]` to the new version. Commit as `chore(release): prepare X.Y.Z`.
3. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
4. The workflow builds the NSIS and MSI installers, signs them if signing secrets are configured (see below), generates `SHA256SUMS.txt` and CycloneDX SBOMs, and creates a **draft** GitHub Release with all artifacts attached.
5. Download the draft installers and smoke-test on a real machine: install, open a real repo, dismiss/restore a flag, mark reviewed, export a report, run `diff-drift check --json`.
6. Paste the changelog section into the release notes and publish the draft. Never publish a release whose installer you have not run.

## Code Signing (not yet configured)

Release binaries are currently **unsigned**: Windows SmartScreen will warn on first run, and some managed environments block unsigned installers outright. Until signing lands, verify downloads against the published `SHA256SUMS.txt`.

The release workflow contains a signing step that runs only when signing secrets exist, so enabling it requires no workflow rewrite. Two supported paths:

- **Azure Trusted Signing** (recommended; subscription-based, no cert file to protect). Configure repo secrets `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, and repo variables `AZURE_TS_ENDPOINT`, `AZURE_TS_ACCOUNT`, `AZURE_TS_PROFILE`, then set the repo variable `WINDOWS_SIGNING=azure`.
- **Authenticode certificate (PFX)**. Configure secrets `WINDOWS_CERT_PFX_B64` (base64 of the .pfx) and `WINDOWS_CERT_PASSWORD`, then set `WINDOWS_SIGNING=pfx`.

With `WINDOWS_SIGNING` unset the step is skipped and the release notes template marks the build unsigned. Do not fake it: never publish an artifact described as signed unless the workflow's verification step (`signtool verify /pa`) passed.

## Reproducible Builds

An org that cannot trust downloaded binaries can build from the tag and compare:

- Toolchains: Node per `package.json` `engines` (CI uses Node 22), Rust stable, Microsoft C++ Build Tools, WebView2.
- Dependencies are fully pinned by `package-lock.json` and `src-tauri/Cargo.lock` (both committed). Use `npm ci`, never `npm install`, for verification builds.
- Build: `npm ci && npm run tauri -- build --bundles nsis,msi --ci`.
- Caveat: NSIS installers embed timestamps, so installer bytes will not be hash-identical across builds. Compare the contained `diff-drift.exe` and resources, or rebuild and diff the bundle directory contents. Byte-for-byte reproducibility is not yet a guarantee. The SBOMs published with each release cover the Rust dependency tree (cargo-cyclonedx) and npm **production** dependencies (`npm sbom --omit dev`) — dev tooling is not included.

## Distribution (winget) — pending

Planned, not published. Prerequisite: signed installers — winget-pkgs moderation and SmartScreen reputation both work badly for unsigned binaries. Once signing lands:

1. Create a manifest under `manifests/s/Statusnone420/DiffDrift/<version>/` in a fork of [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) (`winget create` or `wingetcreate new` against the release URL).
2. The installer URL is the GitHub Release asset; the manifest pins its SHA256 (already published in `SHA256SUMS.txt`).
3. Submit the PR; subsequent versions are a `wingetcreate update --urls <new-asset-url>` away.

Until then, the supported install paths are the GitHub Release installer or building from source.

## Release Checklist (manual, pre-tag)

- Run `npm run test:rust`.
- Run `npm run build`.
- Run `npm run test:e2e:web`.
- Run native E2E for native behavior changes.
- Update `CHANGELOG.md` and align the three version fields.
- After the workflow's draft release: smoke-test the actual installer artifacts (open a real repo, dismiss/restore a flag, mark reviewed, export a report).
- Update README and docs only for behavior that actually exists.
