use crate::{CredentialKind, DesktopSnapshot, FeatureStatus, ProductSurface, ProviderContract};

pub const APP_NAME: &str = "AIUsage";
pub const WINDOWS_PHASE: &str = "Windows Phase A";

pub fn product_surfaces() -> Vec<ProductSurface> {
    [
        ("dashboard", "Dashboard"),
        ("subscriptions", "Subscriptions"),
        ("apiProviders", "API Providers"),
        ("codexProxy", "Codex Proxy"),
        ("opencodeProxy", "OpenCode Proxy"),
        ("claudeProxy", "Claude Code Proxy"),
        ("usageStats", "Usage Stats"),
        ("callAnalytics", "Call Analytics"),
        ("inbox", "Inbox"),
        ("settings", "Settings"),
    ]
    .into_iter()
    .map(|(id, label)| ProductSurface {
        id: id.to_string(),
        label: label.to_string(),
        status: FeatureStatus::FoundationReady,
    })
    .collect()
}

pub fn provider_contracts() -> Vec<ProviderContract> {
    use CredentialKind::*;

    vec![
        ProviderContract {
            id: "codex".into(),
            label: "Codex".into(),
            status: FeatureStatus::FoundationReady,
            credential_kinds: vec![Token, AuthFile],
        },
        ProviderContract {
            id: "copilot".into(),
            label: "Copilot".into(),
            status: FeatureStatus::FoundationReady,
            credential_kinds: vec![Token, OAuth],
        },
        ProviderContract {
            id: "cursor".into(),
            label: "Cursor".into(),
            status: FeatureStatus::FoundationReady,
            credential_kinds: vec![Cookie, WebSession],
        },
        ProviderContract {
            id: "gemini".into(),
            label: "Gemini CLI".into(),
            status: FeatureStatus::FoundationReady,
            credential_kinds: vec![AuthFile, OAuth],
        },
        ProviderContract {
            id: "opencode".into(),
            label: "OpenCode".into(),
            status: FeatureStatus::FoundationReady,
            credential_kinds: vec![ApiKey, AuthFile],
        },
        ProviderContract {
            id: "antigravity".into(),
            label: "Antigravity".into(),
            status: FeatureStatus::Planned,
            credential_kinds: vec![OAuth, WebSession],
        },
        ProviderContract {
            id: "kiro".into(),
            label: "Kiro".into(),
            status: FeatureStatus::Planned,
            credential_kinds: vec![OAuth, AuthFile],
        },
        ProviderContract {
            id: "warp".into(),
            label: "Warp".into(),
            status: FeatureStatus::Blocked,
            credential_kinds: vec![Token, AuthFile],
        },
        ProviderContract {
            id: "droid".into(),
            label: "Droid".into(),
            status: FeatureStatus::Planned,
            credential_kinds: vec![AuthFile, Cookie],
        },
        ProviderContract {
            id: "kimi".into(),
            label: "Kimi".into(),
            status: FeatureStatus::Planned,
            credential_kinds: vec![ApiKey],
        },
        ProviderContract {
            id: "minimax".into(),
            label: "MiniMax".into(),
            status: FeatureStatus::Planned,
            credential_kinds: vec![ApiKey],
        },
    ]
}

pub fn phase_a_snapshot() -> DesktopSnapshot {
    DesktopSnapshot {
        app_name: APP_NAME.into(),
        phase: WINDOWS_PHASE.into(),
        surfaces: product_surfaces(),
        providers: provider_contracts()
            .into_iter()
            .map(|provider| ProductSurface {
                id: provider.id,
                label: provider.label,
                status: provider.status,
            })
            .collect(),
        release_targets: vec![
            "NSIS setup.exe".into(),
            "MSI".into(),
            "Tauri updater".into(),
        ],
    }
}
