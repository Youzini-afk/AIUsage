# ADR 0001: Windows Desktop Stack

## Status

Accepted for Phase A.

## Context

AIUsage is currently a macOS SwiftUI/AppKit application. Windows support requires native Windows packaging, tray behavior, credentials, proxy lifecycle, updater, and installer flows. The existing Swift app depends on Apple frameworks and cannot be reused as a Windows runtime.

## Decision

Create a separate Windows product line in `Windows/` using:

- Tauri 2 for the desktop shell, tray, updater, bundling, and WebView2 host.
- TypeScript/React for the Windows UI.
- Rust crates for product schemas, provider contracts, platform adapters, proxy runtime boundaries, and Tauri command/event integration.
- Windows-specific adapters behind `aiusage-platform` traits.

## Consequences

- macOS and Windows can release independently without weakening native platform behavior.
- Shared behavior must be enforced through schema fixtures and contract tests.
- Provider and proxy logic will be ported deliberately into Rust instead of conditionally compiling Apple-only Swift code.
- Windows CI starts with the foundation workspace and expands into packaging/signing gates in later phases.
