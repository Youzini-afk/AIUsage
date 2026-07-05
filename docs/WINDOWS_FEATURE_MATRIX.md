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
| Call Analytics | Shipped | Full parity | Windows paths and installed-tool discovery required |
| Inbox | Shipped | Full parity | Same message/read-state model |
| Settings | Shipped | Full parity | Windows-specific autostart, tray, paths, update, certificates |
| Tray/menu bar | Shipped | Full parity | Windows tray, context menu, background behavior |
| Auto-update | Sparkle | Tauri updater | Separate signed update feed/metadata |
| Installer | DMG/ZIP | NSIS + MSI | Signed, upgrade-safe, uninstall-safe |

## Current Windows Implementation Checkpoints

| Area | Current status |
| --- | --- |
| Desktop shell | Tauri 2 + React + Rust workspace builds on Windows |
| Contract schema | Core product surfaces, provider identities, proxy tracks, and release targets are covered by fixture tests |
| Platform adapters | Windows app paths, Credential Manager + DPAPI vault, browser profile discovery, system proxy snapshot, and TCP port owner lookup are implemented behind traits |
| Config transforms | Codex `config.toml` managed blocks and OpenCode provider/model injection are implemented in `aiusage-core` |
| Config takeover service | Claude/Codex/OpenCode native Windows paths, custom paths, sidecar `.aiusage.bak` backups, idempotent activation, restore, OpenCode JSONC parsing, and Tauri commands are implemented |

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
| OpenCode local usage | Full | Windows opencode DB path discovery and SQLite access |

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
| Launch at login | Tauri autostart plugin, fallback documented |
| System proxy detection | WinHTTP/WinINET APIs |
| Certificate trust | User certificate store first; admin/local-machine trust as explicit flow |
| Process lifecycle | Rust supervisor, Windows Job Objects if needed |
| Orphan cleanup | Only AIUsage-owned helper/process instances |
| Auto update | Tauri updater, signed metadata |
| Crash/log diagnostics | `%LOCALAPPDATA%\AIUsage\logs` and UI export bundle |

## UX Parity Requirements

- All destructive config operations must show the target file path and backup state.
- Windows/WSL target selection must be explicit for Claude, Codex, and OpenCode.
- Tray background mode must behave predictably: close-to-tray, quit, show window, refresh, and proxy toggles.
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
