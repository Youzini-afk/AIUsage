# AIUsage Windows

Windows is a first-class AIUsage product line built with Tauri 2, Rust, and TypeScript. This workspace is intentionally separate from the existing macOS SwiftUI app so both platforms can keep native release and platform behavior while sharing product contracts.

## Commands

```powershell
pnpm install
pnpm build
cargo test --manifest-path Cargo.toml --workspace
```

For desktop development:

```powershell
pnpm tauri dev
```

## Workspace Shape

```text
Windows/
├── src/                  # TypeScript UI shell
├── src-tauri/            # Tauri desktop entry point
├── crates/
│   ├── aiusage-core      # shared product schemas and domain contracts
│   ├── aiusage-platform  # platform adapter traits
│   ├── aiusage-windows   # Windows adapter implementations
│   ├── aiusage-services  # app services: config takeover, backup/restore, status
│   ├── aiusage-proxy     # proxy runtime boundary
│   ├── aiusage-tauri     # Tauri command/event boundary
│   └── aiusage-fixtures  # fixture and parity-test helpers
└── fixtures/             # schema fixtures used by contract tests
```

The current Windows line includes the app shell, schema/fixture harness, core Windows adapters, sensitive-file permission tightening, Windows Credential Manager-backed credential registry, Claude/Codex/OpenCode config takeover services with backup/restore semantics, an async passthrough proxy supervisor exposed through Tauri commands and UI health, and NSIS/MSI release automation. The proxy runtime emits request/usage events, parses OpenAI/Anthropic token usage shapes, and persists per-track usage archive files. Later phases fill in protocol conversion depth, provider refresh, full UI parity, and installer QA hardening.
