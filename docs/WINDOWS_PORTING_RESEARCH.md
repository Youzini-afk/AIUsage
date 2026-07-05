# Windows Porting Research

This document records the platform research behind a full Windows product line for AIUsage. The scope is product-grade parity.

## Current Project Baseline

AIUsage is currently a macOS-native SwiftUI application with a Swift Package backend.

- `AIUsage.xcodeproj` targets `macosx` with `MACOSX_DEPLOYMENT_TARGET = 14.0`.
- `QuotaBackend/Package.swift` declares `platforms: [.macOS(.v14)]`.
- The release workflow runs on `macos-26` and produces `.app`, `.zip`, and `.dmg` artifacts.
- The macOS app uses platform APIs directly: `SwiftUI`, `AppKit`, `WebKit`, `ServiceManagement`, `SystemConfiguration`, `Security`, `Network.framework`, `Darwin`, `CommonCrypto`, `codesign`, `hdiutil`, `osascript`, `lsof`, and `/usr/bin/security`.

The implication is that Windows support cannot be achieved by adding a packaging flag to the existing Xcode project. It requires a Windows desktop product line with explicit platform adapters.

## Official References

### Desktop Shell And Packaging

- Tauri 2 supports Windows app bundling through NSIS and WiX/MSI. NSIS can generate standard Windows setup installers; MSI support is intended for Windows installer workflows and enterprise deployment. Reference: https://v2.tauri.app/distribute/windows-installer/
- Tauri's sidecar pattern is relevant for bundling and supervising local helper binaries. Reference: https://v2.tauri.app/develop/sidecar/
- Tauri 2 exposes tray/menu integration through its tray APIs. Reference: https://v2.tauri.app/learn/system-tray/
- Tauri updater provides signed update distribution for Tauri apps. Reference: https://v2.tauri.app/plugin/updater/
- Tauri autostart plugin covers launch-at-login behavior. Reference: https://v2.tauri.app/plugin/autostart/
- Electron and electron-builder remain a viable alternative with Windows NSIS/MSI/AppX targets. Reference: https://www.electron.build/win.html

### Windows Security And Platform APIs

- Windows Credential Manager APIs such as `CredWriteW` and `CredReadW` provide native credential storage. Reference: https://learn.microsoft.com/windows/win32/api/wincred/nf-wincred-credwritew
- DPAPI (`CryptProtectData` / `CryptUnprotectData`) provides user-bound local data protection and is the correct primitive for machine-local encrypted blobs. Reference: https://learn.microsoft.com/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata
- Windows certificate stores can be managed through CryptoAPI store functions such as `CertOpenStore` / `CertAddCertificateContextToStore`. Reference: https://learn.microsoft.com/windows/win32/seccrypto/system-store-locations
- WinHTTP can read current user proxy settings through `WinHttpGetIEProxyConfigForCurrentUser`. Reference: https://learn.microsoft.com/windows/win32/api/winhttp/nf-winhttp-winhttpgetieproxyconfigforcurrentuser
- Windows process and TCP table inspection can use IP Helper APIs such as `GetExtendedTcpTable`. Reference: https://learn.microsoft.com/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable
- Windows code signing and timestamping use SignTool. Reference: https://learn.microsoft.com/windows/win32/seccrypto/signtool

### Swift On Windows

- Swift.org publishes Windows toolchains, but this project depends on Apple-only frameworks (`AppKit`, `Network.framework`, `Security`, `SystemConfiguration`, `CommonCrypto`, `Darwin`). Reference: https://www.swift.org/install/windows/
- Therefore, "compile the current Swift app on Windows" is not a viable product strategy. Swift may remain useful for reading or generating parity fixtures, but the Windows runtime should not depend on Apple frameworks.

## Framework Decision

Recommended Windows stack:

- **Desktop shell:** Tauri 2.
- **Core/runtime:** Rust crates.
- **UI:** TypeScript frontend inside Tauri, matching existing AIUsage screens and workflows.
- **Local proxy:** Rust async HTTP runtime (`tokio` + `axum`/`hyper` or equivalent), with streaming/SSE support and Windows TLS integration.
- **Platform APIs:** `windows-rs` for Credential Manager, DPAPI, certificate store, proxy settings, process/TCP inspection, autostart fallback, and shell integration.

Reasoning:

- AIUsage is a tray/background utility with local proxy processes. Tauri provides native tray and installer support with a smaller runtime footprint than Electron.
- Rust is a strong fit for long-running local proxy, file IO, streaming, and platform API bindings.
- The current SwiftUI/AppKit UI cannot be reused on Windows. Rebuilding UI once in Tauri is cleaner than trying to preserve Swift as the Windows application runtime.
- Full product parity is better served by a deliberate platform abstraction layer than by conditional compilation across Apple-only Swift code.

## Platform Equivalence Map

| macOS capability | Current implementation | Windows equivalent |
| --- | --- | --- |
| Credential vault | Keychain via `Security` | Credential Manager for named secrets; DPAPI for local encrypted payloads |
| App preferences | `UserDefaults` | Tauri store or app-owned JSON under `%APPDATA%` |
| Tray menu | `NSStatusBar`, `NSPopover`, `NSMenu` | Tauri tray/menu |
| Windowing | SwiftUI/AppKit | Tauri WebView2 shell |
| Launch at login | `SMAppService` | Tauri autostart plugin, with registry/task scheduler fallback if needed |
| System proxy detection | `SCDynamicStoreCopyProxies` | WinHTTP/WinINET proxy APIs |
| Local helper process | `Process`, macOS process inspection | Rust process supervisor + Windows Job Objects/process APIs |
| Port owner lookup | `lsof`, `proc_pidpath`, `proc_listpids` | `GetExtendedTcpTable` + process image query |
| TLS cert generation/trust | OpenSSL + Keychain trust | Rust/OpenSSL or rcgen + Windows certificate store |
| HTTPS listener | `Network.framework` TLS | Rust HTTP/TLS stack |
| Auto update | Sparkle | Tauri updater |
| Packaging | Xcode + `codesign` + `hdiutil` | Tauri bundler + SignTool + NSIS/MSI |

## Provider-Specific Porting Notes

| Area | Windows consideration |
| --- | --- |
| Codex CLI | Preserve `CODEX_HOME` support and manage `%USERPROFILE%\.codex\config.toml` where native Windows Codex uses it. Keep WSL/native paths separate. |
| Claude Code | Manage `%USERPROFILE%\.claude\settings.json` for native Windows usage. WSL installs should be treated as separate targets, not silently modified. |
| OpenCode | Prefer the official Windows config location if the upstream tool defines one; otherwise support `%USERPROFILE%\.config\opencode\opencode.json[c]` and explicit overrides. |
| Browser sessions | Chromium profile paths differ on Windows and encrypted cookies require Windows DPAPI/AES-GCM handling based on each browser's `Local State`. |
| Cursor/Droid browser cookies | macOS Keychain PBKDF2 logic must be replaced. Windows Chromium cookie decryption is DPAPI-based. |
| Warp | The current provider reads macOS defaults and Keychain; Windows parity needs a separate data source or should be marked unsupported until a Windows source is confirmed. |
| Claude Science | Current integration is macOS desktop-app specific (`osascript`, bundle/runtime paths, Keychain). Windows support requires a distinct Windows Science integration design, not a direct port. |

## Non-Negotiable Product Principles

- Windows parity is a first-class product target with the same release-quality expectations as macOS.
- The Windows build must include installer, updater, signing, tray/background behavior, secure credential storage, config backup/restore, and automated tests.
- Existing macOS release quality must not regress while Windows work proceeds.
- Shared behavior must be validated through fixtures and contract tests, not by manually eyeballing two implementations.
- Any feature that cannot reach parity immediately must have an explicit design note, user-facing state, and follow-up owner.
