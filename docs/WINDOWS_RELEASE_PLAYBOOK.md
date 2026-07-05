# Windows Release Playbook

This playbook tracks the Windows packaging path for the Tauri/Rust product line.

## Release Workflow

The Windows release workflow is `.github/workflows/windows-release.yml`.

It performs:

- version consistency checks across `Windows/package.json`, `Windows/src-tauri/tauri.conf.json`, and `Windows/src-tauri/Cargo.toml`;
- frontend build;
- Rust formatting, clippy, and workspace tests;
- Tauri `nsis` and `msi` bundle builds;
- optional SignTool signing and verification;
- SHA256 checksum generation;
- artifact upload and tag-based GitHub Release asset publishing.

## Required Tools In CI

The workflow runs on `windows-latest` and uses:

- Node.js 24;
- pnpm 11.7.0 via Corepack;
- stable Rust with `rustfmt` and `clippy`;
- Tauri CLI from `Windows/package.json`;
- SignTool from the Windows runner image when signing secrets are present.

## Signing Secrets

Unsigned workflow runs are allowed for non-release validation. Tag releases should provide:

| Secret | Purpose |
| --- | --- |
| `WINDOWS_CERTIFICATE_BASE64` | Base64-encoded `.pfx` code-signing certificate |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the `.pfx` |

The workflow signs:

- the release `.exe` binary under `Windows/src-tauri/target/release`;
- generated NSIS setup `.exe`;
- generated MSI package.

Timestamping uses `http://timestamp.digicert.com` by default.

## Release Artifacts

For version `x.y.z`, the workflow emits:

```text
AIUsage-x.y.z-windows-x64-setup.exe
AIUsage-x.y.z-windows-x64.msi
AIUsage-x.y.z-windows-x64-SHA256SUMS.txt
```

## Manual Verification

Before a public release:

- install the NSIS setup on a clean Windows 11 VM;
- install the MSI on a clean Windows 11 VM;
- launch AIUsage after install;
- confirm Credential Manager entries can be created and deleted;
- activate and restore Claude, Codex, and OpenCode managed configs using test paths first;
- start a proxy runtime against a local test upstream and confirm request/usage archive rows;
- uninstall and confirm app binaries are removed while documented user data remains.
