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
│   ├── aiusage-proxy     # proxy runtime boundary
│   ├── aiusage-tauri     # Tauri command/event boundary
│   └── aiusage-fixtures  # fixture and parity-test helpers
└── fixtures/             # schema fixtures used by contract tests
```

Phase A establishes the app shell, crate boundaries, CI hooks, and schema/fixture harness. Later phases fill in Windows-native credentials, provider refresh, proxy runtime, installer/signing, and release automation.
