use aiusage_core::phase_a_snapshot;
use aiusage_platform::{
    AppPaths, BrowserSessionDiscovery, PortInspector, PortOwner, SystemProxyReader,
};
use aiusage_proxy::{ProxyError, ProxyRuntimeEvent, ProxySupervisor};
use aiusage_services::{
    AppSettingsService, CallAnalyticsInventoryService, CallAnalyticsService, CredentialRegistry,
    DiagnosticsExportService, ManagedConfigService, ServiceError,
};
use aiusage_windows::{
    WindowsAppPaths, WindowsAutostartManager, WindowsBrowserDiscovery, WindowsCredentialVault,
    WindowsFilePermissionGuard, WindowsPortInspector, WindowsSystemProxyReader,
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub use aiusage_core::{DesktopSnapshot, ProxyTrack};
pub use aiusage_proxy::{ProxyHealth, ProxyProtocol, ProxyRuntimeConfig, ProxyRuntimeState};
pub use aiusage_services::{
    AppLanguage, AppSettingsDocument, AppSettingsSnapshot, CallAnalyticsInventorySnapshot,
    CallAnalyticsSnapshot, ClaudeActivationRequest, CodexActivationRequest, CredentialSummary,
    DiagnosticsExportSnapshot, ManagedConfigKind, ManagedConfigStatus, ManagedConfigTargetKind,
    OpenCodeActivationRequest, ProxyUsageArchiveSummary, ProxyUsageStats, ThemeMode,
    UpsertCredentialRequest,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPathSnapshot {
    pub app_config_dir: Option<String>,
    pub app_data_dir: Option<String>,
    pub codex_home: Option<String>,
    pub claude_home: Option<String>,
    pub opencode_config_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TauriDesktopSnapshot {
    #[serde(flatten)]
    pub desktop: DesktopSnapshot,
    pub paths: DesktopPathSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxySummary {
    pub http: Option<String>,
    pub https: Option<String>,
    pub socks: Option<String>,
    pub any_enabled: bool,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileSummary {
    pub browser_name: String,
    pub profile_name: String,
    pub cookies_db_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPortOwnerSummary {
    pub port: u16,
    pub process_id: u32,
    pub image_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPortPreflight {
    pub track: ProxyTrack,
    pub bind_host: String,
    pub port: u16,
    pub available: bool,
    pub owner: Option<ProxyPortOwnerSummary>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformEnvironmentSnapshot {
    pub generated_at_epoch_ms: u128,
    pub paths: DesktopPathSnapshot,
    pub system_proxy: SystemProxySummary,
    pub browser_profiles: Vec<BrowserProfileSummary>,
    pub browser_profile_error: Option<String>,
    pub default_proxy_ports: Vec<ProxyPortPreflight>,
}

pub fn build_phase_a_snapshot() -> DesktopSnapshot {
    phase_a_snapshot()
}

pub fn build_phase_a_desktop_snapshot() -> TauriDesktopSnapshot {
    let paths = WindowsAppPaths::new();
    TauriDesktopSnapshot {
        desktop: build_phase_a_snapshot(),
        paths: desktop_path_snapshot(&paths),
    }
}

pub fn platform_environment() -> PlatformEnvironmentSnapshot {
    let paths = WindowsAppPaths::new();
    let proxy_reader = WindowsSystemProxyReader;
    let system_proxy = match proxy_reader.current_proxy() {
        Ok(snapshot) => SystemProxySummary {
            any_enabled: snapshot.is_any_enabled(),
            http: snapshot.http,
            https: snapshot.https,
            socks: snapshot.socks,
            error_message: None,
        },
        Err(error) => SystemProxySummary {
            http: None,
            https: None,
            socks: None,
            any_enabled: false,
            error_message: Some(error.to_string()),
        },
    };

    let browser_discovery = WindowsBrowserDiscovery::new();
    let (browser_profiles, browser_profile_error) = match browser_discovery.available_profiles() {
        Ok(profiles) => (
            profiles
                .into_iter()
                .map(|profile| BrowserProfileSummary {
                    browser_name: profile.browser_name,
                    profile_name: profile.profile_name,
                    cookies_db_path: profile.cookies_db_path.display().to_string(),
                })
                .collect(),
            None,
        ),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };

    PlatformEnvironmentSnapshot {
        generated_at_epoch_ms: epoch_ms(),
        paths: desktop_path_snapshot(&paths),
        system_proxy,
        browser_profiles,
        browser_profile_error,
        default_proxy_ports: default_proxy_port_preflights(),
    }
}

pub async fn proxy_statuses() -> Vec<ProxyHealth> {
    ensure_proxy_usage_archiver();
    proxy_supervisor().all_health().await
}

pub async fn start_proxy_runtime(config: ProxyRuntimeConfig) -> Result<ProxyHealth, String> {
    ensure_proxy_usage_archiver();
    proxy_supervisor()
        .stop(config.track.clone())
        .await
        .map_err(|error| error.to_string())?;

    let preflight =
        proxy_port_preflight(config.track.clone(), config.bind_host.clone(), config.port);
    if let Some(owner) = &preflight.owner {
        return Err(proxy_port_conflict_message(&preflight, owner));
    }

    proxy_supervisor()
        .start(config)
        .await
        .map_err(|error| error.to_string())
}

pub async fn stop_proxy_runtime(track: ProxyTrack) -> Result<ProxyHealth, ProxyError> {
    ensure_proxy_usage_archiver();
    proxy_supervisor().stop(track).await
}

pub fn proxy_usage_archive_summaries() -> Result<Vec<ProxyUsageArchiveSummary>, ServiceError> {
    proxy_usage_archive_store().summaries()
}

pub fn proxy_usage_stats() -> Result<ProxyUsageStats, ServiceError> {
    proxy_usage_archive_store().usage_stats()
}

pub fn call_analytics_inventory() -> Result<CallAnalyticsInventorySnapshot, ServiceError> {
    CallAnalyticsInventoryService::new(WindowsAppPaths::new()).snapshot()
}

pub fn call_analytics_snapshot() -> Result<CallAnalyticsSnapshot, ServiceError> {
    CallAnalyticsService::new(WindowsAppPaths::new()).snapshot()
}

pub fn app_settings() -> Result<AppSettingsSnapshot, ServiceError> {
    app_settings_service().snapshot()
}

pub fn save_app_settings(
    settings: AppSettingsDocument,
) -> Result<AppSettingsSnapshot, ServiceError> {
    app_settings_service().save(settings)
}

pub fn export_diagnostics() -> Result<DiagnosticsExportSnapshot, ServiceError> {
    DiagnosticsExportService::with_permissions(WindowsAppPaths::new(), WindowsFilePermissionGuard)
        .export()
}

pub fn proxy_port_preflight(track: ProxyTrack, bind_host: String, port: u16) -> ProxyPortPreflight {
    let inspector = WindowsPortInspector;
    proxy_port_preflight_with_inspector(&inspector, track, bind_host, port)
}

pub fn credential_summaries() -> Result<Vec<CredentialSummary>, ServiceError> {
    credential_registry().list_summaries()
}

pub fn save_credential(
    request: UpsertCredentialRequest,
) -> Result<CredentialSummary, ServiceError> {
    credential_registry().upsert(request)
}

pub fn delete_credential(id: String) -> Result<bool, ServiceError> {
    credential_registry().delete(&id)
}

pub fn managed_config_statuses() -> Result<Vec<ManagedConfigStatus>, ServiceError> {
    managed_config_service().statuses()
}

pub fn activate_claude_config(
    request: ClaudeActivationRequest,
) -> Result<ManagedConfigStatus, ServiceError> {
    managed_config_service().activate_claude(request)
}

pub fn restore_claude_config(
    config_path: Option<std::path::PathBuf>,
) -> Result<ManagedConfigStatus, ServiceError> {
    managed_config_service().restore_claude(config_path)
}

pub fn activate_codex_config(
    request: CodexActivationRequest,
) -> Result<ManagedConfigStatus, ServiceError> {
    managed_config_service().activate_codex(request)
}

pub fn restore_codex_config(
    config_path: Option<std::path::PathBuf>,
) -> Result<ManagedConfigStatus, ServiceError> {
    managed_config_service().restore_codex(config_path)
}

pub fn activate_opencode_config(
    request: OpenCodeActivationRequest,
) -> Result<ManagedConfigStatus, ServiceError> {
    managed_config_service().activate_opencode(request)
}

pub fn restore_opencode_config(
    config_path: Option<std::path::PathBuf>,
) -> Result<ManagedConfigStatus, ServiceError> {
    managed_config_service().restore_opencode(config_path)
}

fn managed_config_service() -> ManagedConfigService<WindowsAppPaths, WindowsFilePermissionGuard> {
    ManagedConfigService::with_permissions(WindowsAppPaths::new(), WindowsFilePermissionGuard)
}

fn proxy_supervisor() -> &'static ProxySupervisor {
    static SUPERVISOR: OnceLock<ProxySupervisor> = OnceLock::new();
    SUPERVISOR.get_or_init(ProxySupervisor::new)
}

fn ensure_proxy_usage_archiver() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let mut events = proxy_supervisor().subscribe();
        tokio::spawn(async move {
            let store = proxy_usage_archive_store();
            loop {
                match events.recv().await {
                    Ok(ProxyRuntimeEvent::Usage(usage)) => {
                        if let Err(error) = store.append_usage(usage) {
                            eprintln!("AIUsage proxy usage archive write failed: {error}");
                        }
                    }
                    Ok(ProxyRuntimeEvent::Request(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    });
}

fn proxy_usage_archive_store(
) -> aiusage_services::ProxyUsageArchiveStore<WindowsAppPaths, WindowsFilePermissionGuard> {
    aiusage_services::ProxyUsageArchiveStore::with_permissions(
        WindowsAppPaths::new(),
        WindowsFilePermissionGuard,
    )
}

fn credential_registry() -> CredentialRegistry<WindowsCredentialVault> {
    CredentialRegistry::new(WindowsCredentialVault::default())
}

fn app_settings_service() -> AppSettingsService<WindowsAppPaths, WindowsAutostartManager> {
    AppSettingsService::with_autostart(WindowsAppPaths::new(), WindowsAutostartManager::default())
}

fn default_proxy_port_preflights() -> Vec<ProxyPortPreflight> {
    [
        (ProxyTrack::Codex, 14_399),
        (ProxyTrack::ClaudeCode, 14_400),
        (ProxyTrack::OpenCode, 14_401),
        (ProxyTrack::Global, 14_402),
    ]
    .into_iter()
    .map(|(track, port)| proxy_port_preflight(track, "127.0.0.1".to_string(), port))
    .collect()
}

fn proxy_port_preflight_with_inspector<I>(
    inspector: &I,
    track: ProxyTrack,
    bind_host: String,
    port: u16,
) -> ProxyPortPreflight
where
    I: PortInspector,
{
    if port == 0 {
        return ProxyPortPreflight {
            track,
            bind_host,
            port,
            available: true,
            owner: None,
            error_message: None,
        };
    }

    match inspector.owner_for_port(port) {
        Ok(owner) => {
            let owner = owner.map(port_owner_summary);
            ProxyPortPreflight {
                track,
                bind_host,
                port,
                available: owner.is_none(),
                owner,
                error_message: None,
            }
        }
        Err(error) => ProxyPortPreflight {
            track,
            bind_host,
            port,
            available: false,
            owner: None,
            error_message: Some(error.to_string()),
        },
    }
}

fn port_owner_summary(owner: PortOwner) -> ProxyPortOwnerSummary {
    ProxyPortOwnerSummary {
        port: owner.port,
        process_id: owner.process_id,
        image_path: owner.image_path.map(|path| path.display().to_string()),
    }
}

fn proxy_port_conflict_message(
    preflight: &ProxyPortPreflight,
    owner: &ProxyPortOwnerSummary,
) -> String {
    let image = owner
        .image_path
        .as_deref()
        .map(|path| format!(" ({path})"))
        .unwrap_or_default();
    format!(
        "{}:{} is already in use by process {}{}",
        preflight.bind_host, preflight.port, owner.process_id, image
    )
}

fn desktop_path_snapshot(paths: &WindowsAppPaths) -> DesktopPathSnapshot {
    DesktopPathSnapshot {
        app_config_dir: paths.app_config_dir().ok().map(display_path),
        app_data_dir: paths.app_data_dir().ok().map(display_path),
        codex_home: paths.codex_home().ok().map(display_path),
        claude_home: paths.claude_home().ok().map(display_path),
        opencode_config_dir: paths.opencode_config_dir().ok().map(display_path),
    }
}

fn display_path(path: std::path::PathBuf) -> String {
    path.display().to_string()
}

fn epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiusage_platform::{PlatformError, PlatformResult};
    use std::path::PathBuf;

    #[test]
    fn tauri_snapshot_contains_release_targets() {
        let snapshot = build_phase_a_desktop_snapshot();
        assert!(snapshot
            .desktop
            .release_targets
            .iter()
            .any(|target| target.contains("MSI")));
    }

    #[test]
    fn platform_environment_snapshot_is_non_fatal() {
        let snapshot = platform_environment();
        assert!(snapshot.paths.app_config_dir.is_some());
        let _ = snapshot.system_proxy.any_enabled;
        assert_eq!(snapshot.default_proxy_ports.len(), 4);
    }

    #[test]
    fn proxy_port_preflight_reports_owner() {
        #[derive(Clone, Debug)]
        struct FakePortInspector;

        impl PortInspector for FakePortInspector {
            fn owner_for_port(&self, port: u16) -> PlatformResult<Option<PortOwner>> {
                Ok(Some(PortOwner {
                    port,
                    process_id: 42,
                    image_path: Some(PathBuf::from("C:\\Tools\\server.exe")),
                }))
            }
        }

        let preflight = proxy_port_preflight_with_inspector(
            &FakePortInspector,
            ProxyTrack::Codex,
            "127.0.0.1".into(),
            14_399,
        );
        assert!(!preflight.available);
        assert_eq!(preflight.owner.expect("owner").process_id, 42);
    }

    #[test]
    fn proxy_port_preflight_keeps_inspection_errors_non_fatal() {
        #[derive(Clone, Debug)]
        struct FailingPortInspector;

        impl PortInspector for FailingPortInspector {
            fn owner_for_port(&self, _port: u16) -> PlatformResult<Option<PortOwner>> {
                Err(PlatformError::NotImplemented("test"))
            }
        }

        let preflight = proxy_port_preflight_with_inspector(
            &FailingPortInspector,
            ProxyTrack::Codex,
            "127.0.0.1".into(),
            14_399,
        );
        assert!(preflight.error_message.is_some());
        assert!(preflight.owner.is_none());
    }

    #[test]
    fn managed_config_statuses_resolve_windows_paths() {
        let statuses = managed_config_statuses().expect("status resolution should succeed");
        assert_eq!(statuses.len(), 3);
        assert!(statuses
            .iter()
            .any(|status| status.config_path.contains(".claude")));
        assert!(statuses
            .iter()
            .any(|status| status.config_path.contains(".codex")));
        assert!(statuses
            .iter()
            .any(|status| status.config_path.contains("opencode")));
    }
}
