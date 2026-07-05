use std::{env, path::PathBuf};

use aiusage_platform::{AppPaths, PlatformError, PlatformResult};

#[derive(Clone, Debug, Default)]
pub struct WindowsAppPaths;

impl WindowsAppPaths {
    pub fn new() -> Self {
        Self
    }

    fn env_path(name: &'static str) -> PlatformResult<PathBuf> {
        env::var_os(name)
            .map(PathBuf::from)
            .ok_or(PlatformError::MissingPath(name))
    }

    fn user_profile() -> PlatformResult<PathBuf> {
        Self::env_path("USERPROFILE")
    }
}

impl AppPaths for WindowsAppPaths {
    fn app_config_dir(&self) -> PlatformResult<PathBuf> {
        Ok(Self::env_path("APPDATA")?.join("AIUsage"))
    }

    fn app_data_dir(&self) -> PlatformResult<PathBuf> {
        Ok(Self::env_path("LOCALAPPDATA")?.join("AIUsage"))
    }

    fn app_cache_dir(&self) -> PlatformResult<PathBuf> {
        Ok(Self::env_path("LOCALAPPDATA")?
            .join("AIUsage")
            .join("cache"))
    }

    fn codex_home(&self) -> PlatformResult<PathBuf> {
        Ok(Self::user_profile()?.join(".codex"))
    }

    fn claude_home(&self) -> PlatformResult<PathBuf> {
        Ok(Self::user_profile()?.join(".claude"))
    }

    fn opencode_config_dir(&self) -> PlatformResult<PathBuf> {
        Ok(Self::user_profile()?.join(".config").join("opencode"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiusage_platform::AppPaths;

    #[test]
    fn derives_native_cli_config_paths() {
        let paths = WindowsAppPaths::new();
        let codex = paths.codex_home().expect("USERPROFILE should be available");
        let claude = paths
            .claude_home()
            .expect("USERPROFILE should be available");
        assert!(codex.ends_with(".codex"));
        assert!(claude.ends_with(".claude"));
    }
}
