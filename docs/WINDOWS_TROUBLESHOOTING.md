# Windows Troubleshooting

This guide is for diagnosing Windows-specific AIUsage issues without exposing secrets. Prefer the Settings diagnostics export first, then use the targeted checks below.

## Diagnostics Export

Use **Settings > Export diagnostics** when filing a bug or comparing machines.

The export is written to:

```text
%LOCALAPPDATA%\AIUsage\diagnostics\aiusage-diagnostics-*.json
```

The report contains path existence, file counts, byte totals, recent file metadata, and scan warnings. It does not include raw log bodies, CLI config bodies, credential values, cookies, or API keys.

## App Does Not Launch

Check:

- The installed binary exists under the NSIS/MSI install location.
- WebView2 Runtime is available on the machine.
- `%LOCALAPPDATA%\AIUsage` is writable by the current user.
- Windows Defender or enterprise policy did not quarantine the binary.
- Reinstall with the latest NSIS setup, then retry from Start Menu.

If launch still fails, collect the diagnostics export if possible and capture Windows Event Viewer application errors for `AIUsage`.

## Tray Or Background Mode

Check:

- `Minimize to tray on close` is enabled in Settings.
- `Keep running in background` is enabled when close-to-tray is expected.
- The tray icon appears in the overflow area if Windows hides new tray icons.
- Tray `Quit AIUsage` exits the process; closing the window should not be used as a force-quit path when background mode is enabled.

If the app starts at login unexpectedly, disable `Launch at login` and verify the HKCU Run entry named `AIUsage` is removed:

```powershell
Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" | Select-Object AIUsage
```

## Proxy Port Conflicts

The Windows environment panel shows default proxy port availability:

| Track | Default port |
| --- | --- |
| Codex | `14399` |
| Claude Code | `14400` |
| OpenCode | `14401` |
| Global | `14402` |

If a port is busy, AIUsage reports the owning PID and image path where Windows exposes it.

Manual check:

```powershell
Get-NetTCPConnection -LocalPort 14399 -ErrorAction SilentlyContinue | Select-Object LocalAddress,LocalPort,OwningProcess
```

Then inspect the process:

```powershell
Get-Process -Id <PID> | Select-Object Id,ProcessName,Path
```

Only stop processes you recognize. AIUsage should only stop AIUsage-owned proxy listeners automatically.

## Proxy Requests Fail

Check:

- The proxy track shows `Running` in Runtime health.
- The CLI config points to the expected local base URL and port.
- The request includes the expected client key when the track is configured with one.
- The upstream base URL is reachable from the Windows machine.
- System proxy settings are not forcing loopback traffic through an enterprise proxy.

For a local health probe:

```powershell
Invoke-RestMethod http://127.0.0.1:14399/health
```

Use the track's actual port.

## Local HTTPS CA Issues

AIUsage stores local proxy CA material under:

```text
%APPDATA%\AIUsage\certificates
```

Expected files after `Prepare CA`:

```text
aiusage-local-root-ca.der
aiusage-local-root-ca.pem
aiusage-local-root-ca-key.pem
```

Check CurrentUser Root trust:

```powershell
Get-ChildItem Cert:\CurrentUser\Root | Where-Object Thumbprint -eq "<SHA256_THUMBPRINT>"
```

If trust fails:

- Confirm the UI shows the certificate thumbprint.
- Confirm enterprise policy allows current-user root certificates.
- Confirm the private key file is not shared in bug reports.
- Re-run `Prepare CA` only if the certificate or private key is missing.
- Use `Trust CA` again after policy or permission issues are resolved.

Do not manually import the private key into Windows Root. Only the public certificate should be trusted.

## Credential Manager Issues

AIUsage stores provider credentials in Windows Credential Manager / DPAPI-backed vault data.

Check:

- The current Windows user can open Credential Manager.
- Enterprise policy does not block generic credentials.
- Credential summaries in the UI never show secret values.
- Deleting a credential from AIUsage removes or updates the corresponding vault item.

Do not paste Credential Manager secret blobs into bug reports.

## Config Activation Or Restore

AIUsage uses sidecar backups with `.aiusage.bak`.

Check:

- The UI shows the target config path before activation/restore.
- The Config takeover panel shows `Managed`, `Detected`, `Missing`, or `Warning` for each native target.
- WSL rows appear as `WSL` targets when AIUsage can resolve a distro home path.
- The backup path exists after activation when the original file existed.
- Restore is enabled only when AIUsage detects managed content or a sidecar backup.
- WSL restore remains disabled until explicit WSL activation support is enabled.
- Restore writes the original file bytes back or removes a managed-only file.
- Native Windows and WSL targets are treated as separate choices.

Typical native paths:

```text
%USERPROFILE%\.claude\settings.json
%USERPROFILE%\.codex\config.toml
%USERPROFILE%\.config\opencode\opencode.json
%USERPROFILE%\.config\opencode\opencode.jsonc
```

If a restore fails, stop all related CLIs, copy the `.aiusage.bak` file back to the original path, then retry AIUsage restore.

## WSL And Native Targets

The Windows environment panel separates native Windows paths from WSL distro paths. Native activation should not create or modify files inside WSL distro homes.

Check detected distros:

```powershell
wsl.exe --list --quiet
```

Check a distro home path:

```powershell
wsl.exe -d <DISTRO_NAME> sh -lc 'printf %s "$HOME"'
```

Expected WSL target paths use the distro home:

```text
$HOME/.claude
$HOME/.codex
$HOME/.config/opencode
```

If AIUsage shows a WSL warning:

- Confirm the distro starts outside AIUsage.
- Confirm `wsl.exe` is available on `PATH`.
- Confirm the distro has a POSIX shell at `sh`.
- Treat WSL as unavailable until the warning is resolved; native Windows paths remain usable.

## Call Analytics Empty Or Partial

Check the Windows environment panel and diagnostics export for source paths.

Expected native locations:

```text
%USERPROFILE%\.claude.json
%USERPROFILE%\.claude\settings.json
%USERPROFILE%\.claude\projects
%USERPROFILE%\.codex\config.toml
%USERPROFILE%\.codex\sessions
%USERPROFILE%\.codex\archived_sessions
%USERPROFILE%\.config\opencode\opencode.json
%LOCALAPPDATA%\opencode\opencode.db
```

Malformed JSON/JSONC/TOML should appear as warnings rather than crashing the UI.

## Browser Profile Detection

AIUsage detects browser profile metadata for Chrome, Edge, Brave, and Cursor. The environment panel shows profile names and candidate cookie database paths, but does not read cookie values.

If profiles are missing:

- Confirm the browser has been launched at least once.
- Confirm the profile has a `Cookies` database.
- Check whether an enterprise policy relocates browser user data.
- Close the browser and retry if files are temporarily locked.

## Installer Or Upgrade Issues

Check:

- NSIS and MSI artifacts come from the same version.
- SHA256 checksums match the release file.
- Signed releases pass `signtool verify /pa /v`.
- Upgrade preserves `%APPDATA%\AIUsage`, `%LOCALAPPDATA%\AIUsage`, and Credential Manager entries.
- Uninstall removes app binaries while leaving user data according to the documented policy.

Manual artifact locations after local build:

```text
Windows\target\release\bundle\nsis
Windows\target\release\bundle\msi
```

## Updater Issues

If `Check` reports the updater is unavailable:

- Confirm the installed build was produced with `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PUBLIC_KEY`, and an updater endpoint.
- Confirm the endpoint returns HTTPS in production builds.
- Confirm `latest.json` is reachable from the Windows machine.
- Confirm the `windows-x86_64` entry contains both `url` and inline `signature`.
- Confirm the URL points to the installer bytes that match the `.sig` generated by Tauri.

Expected GitHub Release updater assets:

```text
AIUsage-<version>-windows-x64-setup.exe
AIUsage-<version>-windows-x64-setup.exe.sig
AIUsage-<version>-windows-x64.msi
AIUsage-<version>-windows-x64.msi.sig
latest.json
```

Windows exits AIUsage before applying an update. This is expected for Tauri Windows installers. Relaunch from Start Menu after the installer completes if the app does not reopen automatically.

## What To Include In A Bug Report

Include:

- Windows version and architecture.
- AIUsage version.
- Installer type: NSIS or MSI.
- The diagnostics export JSON.
- The failing workflow and exact timestamp.
- Screenshots of non-secret UI state when relevant.

Do not include:

- API keys, OAuth tokens, cookies, bearer tokens, or Credential Manager secret blobs.
- Full CLI config files unless manually redacted.
- Raw browser cookie databases.
