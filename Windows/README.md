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

The current Windows line includes the app shell, schema/fixture harness, core Windows adapters, and Codex/OpenCode config takeover services with backup/restore semantics. Later phases fill in Claude config takeover, provider refresh, proxy runtime, full UI parity, installer/signing, and release automation.
