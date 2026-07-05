# Windows Porting Plan

This plan describes the work needed to ship a complete Windows adaptation of AIUsage. The work is split into tracks so it can be executed safely, but the target is full product parity.

## Goals

- Ship a first-class Windows desktop app with the same major product capabilities as macOS.
- Preserve macOS release stability while Windows development proceeds.
- Use Windows-native credential, installer, updater, process, and certificate behavior.
- Keep provider/proxy behavior aligned through shared fixtures and contract tests.
- Avoid hidden partial behavior. If a feature is not ready, it must have an explicit status and follow-up task.

## Non-Goals

- Do not attempt to compile the existing SwiftUI/AppKit application as the Windows app.
- Do not ship an unpackaged developer binary as the Windows release.
- Do not store secrets in app-owned plaintext JSON.
- Do not silently edit WSL configuration files when the user selected native Windows, or vice versa.

## Workstreams

### Track A: Product And Architecture Foundation

Deliverables:

- `Windows/` app skeleton with Tauri 2, TypeScript UI, Rust workspace, lint/test commands.
- Architecture Decision Record for Tauri/Rust and platform adapter boundaries.
- Shared schema definitions for provider usage, account credentials, proxy nodes, usage archives, call analytics, and app settings.
- Fixture format for parity tests with macOS.

Completion criteria:

- Windows app launches to the real app shell.
- Rust workspace has core/platform/proxy crate boundaries.
- CI can build and test the Windows tree.

### Track B: Platform Adapters

Deliverables:

- Windows `CredentialVault` using Credential Manager / DPAPI.
- Windows paths service for `%APPDATA%`, `%LOCALAPPDATA%`, native CLI paths, and explicit WSL paths.
- App settings store for non-secret preferences under `%APPDATA%\AIUsage\settings.json`.
- Browser profile discovery for Chrome, Edge, Brave, Cursor, and configurable custom profiles.
- Windows cookie decrypt implementation.
- Process/port inspector using Windows APIs.
- System proxy reader.
- Certificate generation/trust installer.
- Autostart, shell open, notifications, and log export adapters. Launch-at-login now has a Windows HKCU Run adapter wired through Tauri settings commands, and the desktop shell consumes persisted tray/background lifecycle settings.

Completion criteria:

- Adapter test suite passes on `windows-latest`.
- Manual Windows QA verifies credential prompts, browser profile discovery, port conflict UI, and certificate trust flow.

### Track C: Core Domain And Provider Parity

Deliverables:

- Provider models and normalization logic ported to Rust.
- Account registry and credential reference model.
- Provider refresh coordinator equivalent.
- API providers and distribution model.
- Pricing/currency conversion.
- Usage archive readers/writers.
- Call analytics inventory and aggregation. The Windows foundation now detects native Claude/Codex/OpenCode configs, session stores, user skill roots, configured MCP servers, and aggregates Claude/Codex/OpenCode call events into a Tauri snapshot.

Completion criteria:

- Fixture parity tests pass against representative Swift outputs.
- All current providers have a Windows implementation status in the feature matrix.
- No provider writes credentials outside the credential vault except when required by the target CLI config format.

### Track D: Proxy Runtime

Deliverables:

- Rust proxy runtime for Claude, Codex, OpenCode, and global proxy tracks.
- Streaming/SSE support and upstream error handling.
- Local proxy request accounting.
- Per-node log ingestion and archive writes.
- Proxy supervisor with start/stop/restart/connectivity test APIs.
- TLS local proxy support.

Completion criteria:

- Protocol tests cover conversion, passthrough, streaming, and failure responses.
- All proxy modes have config activation and restore tests.
- Windows firewall/certificate behavior is documented and QA-tested.

### Track E: Windows UI

Deliverables:

- Dashboard.
- Subscriptions and account editor.
- API Providers.
- Claude Code Proxy.
- Codex Proxy.
- OpenCode Proxy.
- Usage Stats.
- Call Analytics.
- Inbox.
- Settings.
- Tray menu and background behavior. The current Tauri shell implements Show AIUsage, Open Settings, Quit AIUsage, click-to-restore, close-to-tray, and explicit quit bypass; tray refresh/proxy shortcuts remain part of UI parity work.

Completion criteria:

- UI routes map to all macOS product surfaces.
- Major workflows are covered by automated E2E tests and manual QA checklist.
- Long-running refresh/proxy operations stream progress and errors to the UI.

### Track F: Packaging, Signing, Updates, CI

Deliverables:

- `release-windows.yml` on GitHub Actions.
- Tauri NSIS setup installer.
- Tauri MSI installer.
- SignTool signing and timestamping.
- Tauri updater metadata and signed update artifacts.
- Release asset naming and GitHub Release upload.
- Installer upgrade/uninstall tests.

Completion criteria:

- Fresh install, upgrade, uninstall, and update are tested on clean Windows VMs.
- Binaries and installers are signed.
- Windows release artifacts are published alongside macOS artifacts.

### Track G: Migration, QA, And Documentation

Deliverables:

- Windows user docs.
- Admin/enterprise install notes.
- Troubleshooting docs for proxy ports, certificate trust, Defender/firewall, WSL/native target selection, and provider login.
- Export/import flow for moving non-secret settings and optionally encrypted credentials.
- Manual QA matrix for Windows 10/11, x64, and later arm64.

Completion criteria:

- Documentation covers every first-run and recovery path.
- QA sign-off covers provider refresh, proxy activation, tray behavior, installer, updater, and credential storage.

## Milestones

| Milestone | Outcome |
| --- | --- |
| M1 Architecture freeze | Tauri/Rust skeleton, adapter traits, schemas, and CI baseline |
| M2 Platform-ready app | Windows credentials, paths, tray, autostart, process/port, proxy settings, certificate trust |
| M3 Provider parity | Provider refresh, accounts, API providers, usage normalization, archives, call analytics |
| M4 Proxy parity | Claude/Codex/OpenCode/global proxy tracks with config takeover and accounting |
| M5 UI parity | All product screens and tray workflows implemented |
| M6 Release readiness | Signed NSIS/MSI, updater, installer QA, docs, migration, release checklist |

## Suggested Implementation Order

1. Add `Windows/` Tauri/Rust workspace and CI.
2. Define shared schemas and fixture harness.
3. Implement Windows platform adapters before feature UI.
4. Port config takeover logic for Codex, Claude, and OpenCode with tests.
5. Build proxy runtime and accounting.
6. Port provider refresh and normalization.
7. Build UI routes and tray workflows.
8. Add installer/updater/signing and release checks.
9. Run full QA and close provider-specific gaps.

## Engineering Standards

- Every platform-specific call must sit behind a named adapter.
- Every managed config write must have a restore test.
- Every secret has an owner and storage decision.
- Every provider has fixture coverage or a documented blocker.
- Every proxy mode has streaming tests.
- CI must reject unsigned release builds.
- Docs and UI must distinguish native Windows from WSL targets.

## Release Checklist

- Version is synchronized across Windows app metadata and GitHub release.
- NSIS installer builds and installs per-user.
- MSI builds and installs in admin/enterprise path.
- SignTool signature and timestamp verified.
- Tauri update metadata generated and signed.
- Installed app launches after fresh install and after upgrade.
- Tray icon/menu works after reboot when autostart is enabled, including close-to-tray, restore, Open Settings, and explicit Quit.
- Credential vault survives upgrade.
- Proxy activation/restoration tested for Claude, Codex, OpenCode, and global proxy.
- Logs and diagnostics can be exported.
- Uninstall removes app binaries and leaves user data according to documented policy.

## Open Questions

| Question | Owner/action |
| --- | --- |
| Which upstream Windows config paths are official for OpenCode and Claude Code native installs? | Inventory currently covers `%USERPROFILE%\.claude.json`, `%USERPROFILE%\.claude\settings.json`, `%USERPROFILE%\.config\opencode\opencode.json[c]`, `%LOCALAPPDATA%\opencode\opencode.db`, and `%USERPROFILE%\.local\share\opencode\opencode.db`; confirm against real installs before release |
| Does Warp expose Windows usage/account data comparable to macOS defaults? | Research provider data source before committing full parity |
| Should the first Windows release include arm64? | Decide after x64 release pipeline is stable |
| Should certificate trust default to user store only? | Prototype UX and test with target CLIs |
| Should the proxy run in-process or sidecar by default? | Start with in-process service plus sidecar CLI option; revisit after crash-isolation testing |

## Tracking Artifacts

- `docs/WINDOWS_PORTING_RESEARCH.md`
- `docs/WINDOWS_PORTING_ARCHITECTURE.md`
- `docs/WINDOWS_FEATURE_MATRIX.md`
- Future: `Windows/README.md`
- Future: `docs/WINDOWS_RELEASE_PLAYBOOK.md`
- Future: `docs/WINDOWS_QA_CHECKLIST.md`
