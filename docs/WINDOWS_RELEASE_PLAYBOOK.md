# Windows Release Playbook

This playbook tracks the Windows packaging path for the Tauri/Rust product line.

The manual release gate is [Windows QA Checklist](WINDOWS_QA_CHECKLIST.md).

## Release Workflow

The Windows release workflow is `.github/workflows/windows-release.yml`.

It performs:

- version consistency checks across `Windows/package.json`, `Windows/src-tauri/tauri.conf.json`, and `Windows/src-tauri/Cargo.toml`;
- frontend build;
- Rust formatting, clippy, and workspace tests;
- Tauri `nsis` and `msi` bundle builds;
- optional Windows code signing during Tauri bundling;
- optional Tauri updater artifact signing and `latest.json` manifest generation;
- SignTool verification when a code-signing certificate is configured;
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

Unsigned workflow runs are allowed for non-release validation. Tag releases should provide Windows code-signing material:

| Secret | Purpose |
| --- | --- |
| `WINDOWS_CERTIFICATE_BASE64` | Base64-encoded `.pfx` code-signing certificate |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the `.pfx` |

The workflow imports the certificate into the runner certificate store, injects its thumbprint into a temporary Tauri release config, and lets Tauri sign during bundle creation. This keeps updater signatures valid because the updater `.sig` files are generated after the installer bytes are finalized.

Timestamping uses `http://timestamp.digicert.com` by default.

## Updater Secrets And Variables

Tauri updater signing is separate from Windows Authenticode signing. Generate an updater key pair with:

```powershell
pnpm --dir Windows tauri signer generate -- -w "$env:USERPROFILE\.tauri\aiusage-windows.key"
```

Configure release automation with:

| Name | Kind | Purpose |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Secret | Private updater signing key content or path |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Secret | Optional updater private key password |
| `TAURI_SIGNING_PUBLIC_KEY` | Variable or secret | Public updater key embedded into the release config |
| `WINDOWS_UPDATE_ENDPOINT` | Variable | Optional endpoint used by the installed app |
| `WINDOWS_UPDATE_BASE_URL` | Variable | Optional base URL used when generating `latest.json` download URLs |

If `WINDOWS_UPDATE_ENDPOINT` is not set, the workflow uses:

```text
https://github.com/<owner>/<repo>/releases/latest/download/latest.json
```

On tag builds, if `WINDOWS_UPDATE_BASE_URL` is not set, `latest.json` points at that tag's GitHub Release assets. The updater manifest uses the `windows-x86_64` platform key and the NSIS setup asset as the install URL.

## Release Artifacts

For version `x.y.z`, the workflow emits:

```text
AIUsage-x.y.z-windows-x64-setup.exe
AIUsage-x.y.z-windows-x64.msi
AIUsage-x.y.z-windows-x64-SHA256SUMS.txt
```

When updater signing is enabled, it also emits:

```text
AIUsage-x.y.z-windows-x64-setup.exe.sig
AIUsage-x.y.z-windows-x64.msi.sig
latest.json
```

## Manual Verification

Before a public release:

- install the NSIS setup on a clean Windows 11 VM;
- install the MSI on a clean Windows 11 VM;
- launch AIUsage after install;
- confirm the Windows environment panel shows app paths, system proxy state, browser profile status, and default proxy port availability;
- verify tray menu Show AIUsage, Open Settings, Quit AIUsage, close-to-tray, and restore behavior;
- export diagnostics from Settings and confirm the report is metadata-only;
- confirm Credential Manager entries can be created and deleted;
- activate and restore Claude, Codex, and OpenCode managed configs using test paths first;
- start a proxy runtime against a local test upstream and confirm request/usage archive rows;
- confirm the release contains `.sig` files and `latest.json` when updater secrets are configured;
- install the previous signed version, run `Check` in the Windows artifacts panel, install the update, and confirm the app exits and upgrades successfully;
- uninstall and confirm app binaries are removed while documented user data remains.
