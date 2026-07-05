# Windows QA Checklist

This checklist is the manual release gate for the Windows Tauri/Rust product line. It is intentionally broader than a smoke test: a Windows build is not release-ready until each relevant section is either passed or marked with an explicit blocker.

## Test Matrix

| Dimension | Required coverage |
| --- | --- |
| OS | Windows 11 current stable, Windows 10 22H2 |
| Architecture | x64 for first release; arm64 only after x64 is stable |
| Install mode | NSIS per-user setup, MSI enterprise/admin install |
| Upgrade mode | Previous signed build to current signed build |
| User state | Clean profile, existing `%APPDATA%\AIUsage`, existing CLI configs |
| WSL state | WSL not installed, WSL installed with at least one stopped/started distro |
| Network | No system proxy, system proxy enabled, offline upstream |

## Preflight

- Confirm versions match in `Windows/package.json`, `Windows/src-tauri/tauri.conf.json`, and `Windows/src-tauri/Cargo.toml`.
- Build locally with `pnpm --dir Windows build`.
- Run `cargo test --manifest-path Windows/Cargo.toml --workspace`.
- Run `cargo clippy --manifest-path Windows/Cargo.toml --workspace --all-targets -- -D warnings`.
- Build installers with `pnpm --dir Windows tauri build --bundles nsis,msi`.
- Confirm the release binary and both bundles exist under `Windows/target/release`.
- For signed release rehearsal, confirm `WINDOWS_CERTIFICATE_BASE64`, `TAURI_SIGNING_PRIVATE_KEY`, and `TAURI_SIGNING_PUBLIC_KEY` are configured in CI.

## Install And Launch

- Install NSIS setup on a clean Windows VM.
- Install MSI on a clean Windows VM.
- Launch AIUsage from Start Menu and installed binary path.
- Confirm WebView2 loads the app shell without a blank screen.
- Confirm the Windows environment panel shows app paths, system proxy state, browser profile status, and default proxy port availability.
- Confirm the Windows environment panel shows Local HTTPS CA state and does not create or trust a CA until the explicit CA action is used.
- On a machine without WSL, confirm the Windows environment panel shows a normal WSL empty state and no app error.
- On a machine with WSL, confirm each distro row shows the distro name plus Claude, Codex, and OpenCode target paths.
- Confirm no unexpected console window appears during normal GUI launch.

## Tray And Lifecycle

- Close the main window with default settings and confirm the app stays in the tray.
- Left-click or double-click the tray icon and confirm the main window restores and focuses.
- Use tray menu `Show AIUsage` and confirm the main window restores.
- Use tray menu `Open Settings` and confirm the Settings panel is selected.
- Use tray menu `Quit AIUsage` and confirm the process exits instead of returning to tray.
- Enable `Launch at login`, reboot, and confirm AIUsage starts and tray behavior remains available.
- Disable `Launch at login`, reboot, and confirm the HKCU Run entry is removed.

## Settings And Diagnostics

- Change theme, language, refresh interval, proxy restore, close-to-tray, and background settings; restart and confirm persistence.
- Confirm `%APPDATA%\AIUsage\settings.json` exists and contains no secrets.
- Export diagnostics from Settings.
- Confirm the diagnostics file is created under `%LOCALAPPDATA%\AIUsage\diagnostics`.
- Confirm the diagnostics report contains path/file metadata and does not contain raw log bodies, credential values, cookies, or API keys.

## Credential Vault

- Add an API key credential through the Windows UI or command path.
- Confirm the UI only shows secret-free summaries.
- Confirm the credential is stored in Windows Credential Manager.
- Delete the credential and confirm the Credential Manager entry updates.
- Upgrade the app and confirm existing credentials remain readable.

## Config Takeover

- Test Claude native Windows config activation against a temporary `settings.json`; confirm backup sidecar is created.
- Test Claude restore; confirm original file bytes are restored or managed-only file is removed.
- Test Codex native Windows `config.toml` activation; confirm TOML user content is preserved outside managed blocks.
- Test Codex restore; confirm original TOML bytes are restored.
- Test OpenCode JSON and JSONC activation; confirm comments/trailing commas survive backup/restore.
- Confirm all destructive operations show target path and backup state in UI.
- Confirm native Windows and WSL targets are never silently mixed.
- Confirm current native activation does not create or modify files under WSL distro homes.

## Proxy Runtime

- Start each proxy track with a local test upstream.
- Confirm `/health` returns the correct track and listening port.
- Send authorized and unauthorized requests; confirm client-key enforcement.
- Confirm OpenAI and Anthropic usage shapes are parsed into archive rows.
- Confirm streaming/SSE responses pass through.
- Occupy a default proxy port with another process and confirm Windows port preflight shows `Busy` plus owning PID/path where available.
- Attempt to start a proxy on the occupied port and confirm the error names the owner instead of only showing a bind failure.
- Stop each proxy track and confirm only AIUsage-owned listeners are stopped.

## Local HTTPS CA

- Use `Prepare CA` and confirm AIUsage creates certificate files under `%APPDATA%\AIUsage\certificates`.
- Confirm `aiusage-local-root-ca-key.pem` is ACL-restricted to the current user, SYSTEM, and Administrators.
- Confirm the UI shows a SHA-256 thumbprint after preparation.
- Use `Trust CA` and confirm the certificate appears in `Cert:\CurrentUser\Root`.
- Confirm `Trust CA` is disabled after the CurrentUser Root trust check succeeds.
- Confirm diagnostics export never includes private-key contents.

## Usage And Call Analytics

- Generate proxy usage and confirm `%APPDATA%\AIUsage\usage-archive\proxy-usage-<track>-v1.json` updates.
- Confirm the Usage Stats panel aggregates request, input, output, cache, track, and model totals.
- Seed Claude/Codex JSONL and OpenCode SQLite fixtures under Windows paths.
- Confirm Call Analytics inventory detects configs, sessions, skills, and MCP servers.
- Confirm Call Analytics aggregation reports MCP, Skill, Tool counts, success signals, and duration where source data supports it.
- Confirm missing or malformed source files produce warnings instead of a blank or crashed UI.

## Browser And Provider Inputs

- Confirm Chrome, Edge, Brave, and Cursor profiles are detected when present.
- Confirm browser profile rows do not expose cookie values.
- Confirm missing browsers are treated as a normal empty state.
- Confirm provider features without Windows support show explicit status rather than silent empty data.

## Installer, Upgrade, Uninstall

- Install over a previous signed build and confirm app state, settings, credentials, and usage archives remain.
- Uninstall NSIS build and confirm binaries are removed.
- Uninstall MSI build and confirm binaries are removed.
- Confirm user data retention/removal matches the documented policy.
- Verify signed artifacts with SignTool when signing secrets are configured.
- Confirm SHA256 checksums match published release assets.

## Updater

- Confirm tag release artifacts include `AIUsage-<version>-windows-x64-setup.exe.sig`, `AIUsage-<version>-windows-x64.msi.sig`, and `latest.json` when updater secrets are configured.
- Confirm `latest.json` contains a valid `windows-x86_64` platform entry, an HTTPS URL, and an inline signature string.
- Install the previous signed Windows release.
- Use the Windows artifacts panel `Check` action and confirm the new version is detected.
- Use `Install` and confirm Windows exits AIUsage before installer handoff.
- Relaunch after update and confirm the installed version, settings, credentials, usage archives, and tray behavior survived the upgrade.
- Confirm offline or unavailable updater endpoint errors are shown in the UI without blocking normal app launch.

## Blocker Rules

- Any plaintext secret in app-owned JSON is a release blocker.
- Any config takeover path that cannot restore original bytes is a release blocker.
- Any tray `Quit` action that leaves the app running is a release blocker.
- Any installer that cannot upgrade without losing credentials/settings is a release blocker.
- Any updater-enabled release missing `.sig` files or a valid `latest.json` is a release blocker.
- Any local CA flow that creates a private key without restrictive ACLs is a release blocker.
- Any proxy mode that loses usage accounting for normal success responses is a release blocker.
- Any UI route that silently hides unsupported provider state is a release blocker.
- Any WSL detection failure that breaks the Windows environment snapshot is a release blocker.
