use aiusage_core::phase_a_snapshot;
use aiusage_platform::AppPaths;
use aiusage_proxy::{ProxyError, ProxySupervisor};
use aiusage_services::{ManagedConfigService, ServiceError};
use aiusage_windows::{WindowsAppPaths, WindowsFilePermissionGuard};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub use aiusage_core::{DesktopSnapshot, ProxyTrack};
pub use aiusage_proxy::{ProxyHealth, ProxyProtocol, ProxyRuntimeConfig, ProxyRuntimeState};
pub use aiusage_services::{
    ClaudeActivationRequest, CodexActivationRequest, ManagedConfigKind, ManagedConfigStatus,
    ManagedConfigTargetKind, OpenCodeActivationRequest,
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

pub fn build_phase_a_snapshot() -> DesktopSnapshot {
    phase_a_snapshot()
}

pub fn build_phase_a_desktop_snapshot() -> TauriDesktopSnapshot {
    let paths = WindowsAppPaths::new();
    TauriDesktopSnapshot {
        desktop: build_phase_a_snapshot(),
        paths: DesktopPathSnapshot {
            app_config_dir: paths.app_config_dir().ok().map(display_path),
            app_data_dir: paths.app_data_dir().ok().map(display_path),
            codex_home: paths.codex_home().ok().map(display_path),
            claude_home: paths.claude_home().ok().map(display_path),
            opencode_config_dir: paths.opencode_config_dir().ok().map(display_path),
        },
    }
}

pub async fn proxy_statuses() -> Vec<ProxyHealth> {
    proxy_supervisor().all_health().await
}

pub async fn start_proxy_runtime(config: ProxyRuntimeConfig) -> Result<ProxyHealth, ProxyError> {
    proxy_supervisor().start(config).await
}

pub async fn stop_proxy_runtime(track: ProxyTrack) -> Result<ProxyHealth, ProxyError> {
    proxy_supervisor().stop(track).await
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

fn display_path(path: std::path::PathBuf) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
