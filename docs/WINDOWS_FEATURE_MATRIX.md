# Windows Feature Matrix

This matrix tracks target parity for the Windows product line. "Target" means the intended shipped behavior, not current implementation status.

## Product Surfaces

| Surface | macOS status | Windows target | Notes |
| --- | --- | --- | --- |
| Dashboard | Shipped | Full parity | Same provider cards, overview, refresh state, alert state |
| Subscriptions | Shipped | Full parity | Multi-account management and activation state preserved |
| API Providers | Shipped | Full parity | Shared upstream configs and distribution into proxy tracks |
| Claude Code Proxy | Shipped | Full parity | Native Windows `.claude` target plus explicit WSL mode |
| Codex Proxy | Shipped | Full parity | Native `%USERPROFILE%\.codex` target plus `CODEX_HOME` launch command |
| OpenCode Proxy | Shipped | Full parity | Native/fallback config path plus JSON/JSONC takeover |
| Global Proxy | Shipped | Full parity | Same fixed-entry hot-switch model |
| Usage Stats | Shipped | Full parity | Same archive semantics and cost freezing |
| Call Analytics | Shipped | Full parity | Native Windows inventory and event snapshot now cover Claude/Codex JSONL plus OpenCode SQLite tool parts; advanced ranking/range UX remains in progress |
| Inbox | Shipped | Full parity | Same message/read-state model |
| Settings | Shipped | Full parity | Windows-specific autostart, tray, paths, update, certificates |
| Tray/menu bar | Shipped | Full parity | Windows tray, context menu, background behavior |
| Auto-update | Sparkle | Tauri updater | Signed update artifacts, static manifest, and in-app check/install UI |
| Installer | DMG/ZIP | NSIS + MSI | Signed, upgrade-safe, uninstall-safe |

## Current Windows Implementation Checkpoints

| Area | Current status |
| --- | --- |
| Desktop shell | Tauri 2 + React + Rust workspace builds on Windows |
| Contract schema | Core product surfaces, provider identities, proxy tracks, and release targets are covered by fixture tests |
| Platform adapters | Windows app paths, Credential Manager + DPAPI vault, browser profile discovery, system proxy snapshot, TCP port owner lookup, and sensitive-file permission tightening are implemented behind traits |
| Platform environment UI | Tauri exposes Windows app/CLI paths, current-user WinHTTP/WinINET proxy snapshot, Chromium/Cursor browser profile discovery, and default proxy port preflight; the Windows UI summarizes proxy state, known paths, endpoint count, detected profiles, and free/busy default ports without reading cookies |
| Config transforms | Codex `config.toml` managed blocks and OpenCode provider/model injection are implemented in `aiusage-core` |
| Config takeover service | Claude/Codex/OpenCode native Windows paths, custom paths, sidecar `.aiusage.bak` backups, idempotent activation, restore, OpenCode JSONC parsing, and Tauri commands are implemented |
| Proxy runtime | Async Rust passthrough supervisor starts/stops local listeners, exposes health through Tauri, validates client keys, normalizes `/v1` upstream paths, injects upstream auth, streams upstream responses, emits request/usage events, parses OpenAI/Anthropic usage shapes, and surfaces four-track health in the Windows UI |
| Proxy port preflight | Windows Tauri layer checks requested proxy ports after stopping the same track and before binding; external owners are reported with PID/path where available, and a reusable `proxy_port_preflight` command supports custom node ports |
| Usage archive | Proxy usage events are persisted per track under `%APPDATA%\AIUsage\usage-archive\proxy-usage-<track>-v1.json`, permission-tightened through the Windows file guard, and summarized in the Windows UI |
| Usage stats | Proxy usage archives aggregate into request/input/output/cache token totals with track/model breakdowns exposed through Tauri and displayed in the Windows UI |
| Call Analytics inventory | Windows service scans `%USERPROFILE%\.claude.json`, `%USERPROFILE%\.claude\settings.json`, `%USERPROFILE%\.claude\projects`, `%USERPROFILE%\.codex\config.toml`, `%USERPROFILE%\.codex\sessions`, `%USERPROFILE%\.codex\archived_sessions`, `%USERPROFILE%\.config\opencode\opencode.json[c]`, OpenCode data DB candidates, and user skill roots; results are exposed through Tauri and summarized in the Windows UI |
| Call Analytics aggregation | Windows snapshot service aggregates Claude `tool_use`/`tool_result`, Codex `function_call`/`mcp_tool_call_end`, and OpenCode `part` table tool rows into day/source/kind/name entries with success and duration signals where available |
| Credential registry | Structured provider credentials are stored in Windows Credential Manager/DPAPI through the platform vault, exposed as secret-free summaries through Tauri, and counted in the Windows UI |
| App settings | `%APPDATA%\AIUsage\settings.json` stores non-secret Windows preferences, Tauri commands expose load/save, launch-at-login syncs with the HKCU Run registry key, and tray lifecycle flags are consumed by the desktop shell |
| Tray lifecycle | Tauri tray-icon support is enabled; the tray menu exposes Show AIUsage, Open Settings, and Quit AIUsage; left click/double click restores the main window; close hides the main window when `minimizeToTrayOnClose` or `keepRunningInBackground` is enabled; explicit Quit bypasses close-to-tray |
| Diagnostics export | Settings can export a secret-free diagnostics metadata JSON under `%LOCALAPPDATA%\AIUsage\diagnostics`, covering app/log/archive path status, file counts, byte totals, recent file metadata, and scan warnings without raw log/config contents |
| Packaging | Windows release workflow builds NSIS and MSI bundles, optionally injects Windows code signing into Tauri bundling, optionally emits updater `.sig` files plus `latest.json`, verifies Authenticode signatures, emits SHA256 checksums, uploads artifacts, and publishes tag release assets |

## Provider Matrix

| Provider | Windows parity target | Windows-specific work |
| --- | --- | --- |
| Codex | Full | Native `.codex` config/auth paths, `CODEX_HOME`, no-proxy environment handling, WSL target selection |
| Copilot | Full | GitHub device flow, `gh` discovery on Windows, token storage in Credential Manager |
| Cursor | Full | Windows Chromium/Cursor profile discovery and DPAPI cookie decrypt |
| Antigravity | Full where Windows client/source exists | OAuth/device/local auth discovery needs Windows source confirmation |
| Kiro | Full | AWS SSO/device flow and Windows IDE cache discovery |
| Warp | Full parity after Windows source confirmation | Current macOS defaults/Keychain source has no direct Windows equivalent yet; requires Windows data-source research |
| Gemini CLI | Full | Windows CLI auth file discovery and browser loopback behavior |
| Droid | Full | Windows auth file discovery and DPAPI cookie decrypt |
| Kimi | Full | API key/config discovery under Windows user paths |
| MiniMax | Full | API key/subscription key storage and refresh |
| Claude Code local usage | Full | Windows `.claude` paths, logs, managed settings, proxy archive |
| Codex cost/local usage | Full | Native and WSL session log discovery with explicit target |
| OpenCode local usage | Full | Windows OpenCode DB path discovery and SQLite tool-event reader are present; cost/account usage reader remains separate provider work |

## Proxy Feature Matrix

| Feature | Windows target |
| --- | --- |
| Claude OpenAI conversion | Full parity |
| Anthropic passthrough | Full parity |
| Codex Responses passthrough | Full parity |
| OpenCode direct mode | Full parity |
| OpenCode proxy mode | Full parity |
| Global proxy hot switch | Full parity |
| Per-node request log | Full parity |
| Per-node usage archive | Full parity |
| TLS local proxy | Full parity with Windows cert store |
| Port conflict detection | Full parity with owning process details |
| Proxy-only mode | Full parity |
| Connectivity tests | Full parity |

## Platform Feature Matrix

| Capability | Windows implementation target |
| --- | --- |
| Secure credential vault | Credential Manager + DPAPI |
| Secret-bearing config hygiene | Store references where possible, protect local blobs, avoid plaintext secrets outside tool-required config files |
| App data paths | `%APPDATA%` for durable config; `%LOCALAPPDATA%` for logs/cache/runtime |
| Restrictive file behavior | Windows ACL/security descriptor decisions instead of POSIX `0600` |
| Browser opening | Tauri opener / Windows shell |
| Web login | System browser first; embedded WebView only when product requirements demand it |
| Notifications | Tauri notification plugin or Windows notification integration |
| Launch at login | HKCU Run registry adapter implemented and wired to Windows settings |
| System proxy detection | WinHTTP/WinINET APIs |
| Certificate trust | User certificate store first; admin/local-machine trust as explicit flow |
| Process lifecycle | Rust supervisor, Windows Job Objects if needed |
| Orphan cleanup | Only AIUsage-owned helper/process instances |
| Auto update | Tauri updater plugin, signed artifacts, static `latest.json`, Windows passive install mode |
| Crash/log diagnostics | `%LOCALAPPDATA%\AIUsage\logs`, `%LOCALAPPDATA%\AIUsage\proxy-logs`, and Settings diagnostics metadata export |

## UX Parity Requirements

- All destructive config operations must show the target file path and backup state.
- Windows/WSL target selection must be explicit for Claude, Codex, and OpenCode.
- Tray background mode must behave predictably: close-to-tray, quit, show window, refresh, and proxy toggles. Current shell support covers close-to-tray, show window, open Settings, and explicit quit; refresh/proxy tray shortcuts remain UX follow-up work.
- Update notifications should be non-modal and match the current gentle-reminder philosophy.
- Missing provider support must be visible as a clear status, not as silent empty data.

## Release Blockers

Windows release is not ready until:

- Installer and updater are signed and tested.
- Credential vault round trips secrets without plaintext app-owned storage.
- Config activation and restore pass destructive-path tests for Claude, Codex, and OpenCode.
- Proxy tracks pass streaming and accounting tests.
- Browser/Cookie import behavior is tested for Chrome, Edge, Brave, and Cursor.
- Native/WSL target handling is covered by tests and UX copy.
- Upgrade from one signed Windows version to the next preserves app state and credentials.
