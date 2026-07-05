use aiusage_core::phase_a_snapshot;
use aiusage_platform::AppPaths;
use aiusage_windows::WindowsAppPaths;
use serde::{Deserialize, Serialize};

pub use aiusage_core::DesktopSnapshot;

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
}
