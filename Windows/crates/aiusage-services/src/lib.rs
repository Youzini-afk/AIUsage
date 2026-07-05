use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use aiusage_core::{
    claude_has_managed_entries, inject_claude_managed_settings, inject_codex_managed_config,
    inject_opencode_managed_config_with_base, opencode_has_managed_entries, parse_json_or_jsonc,
    strip_claude_managed_settings, strip_codex_managed_blocks, strip_opencode_managed_entries,
    ClaudeManagedSettings, CodexManagedConfig, OpenCodeManagedNode,
};
use aiusage_platform::{AppPaths, FilePermissionGuard, PlatformError, PlatformResult};
use aiusage_proxy::ProxyUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error("managed config IO failed: {0}")]
    Io(#[from] io::Error),
    #[error("managed config JSON parse failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("managed config path is not valid UTF-8: {0:?}")]
    NonUtf8Path(PathBuf),
    #[error("managed config request is invalid: {0}")]
    InvalidRequest(&'static str),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

pub const PROXY_USAGE_ARCHIVE_VERSION: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct NoopFilePermissionGuard;

impl FilePermissionGuard for NoopFilePermissionGuard {
    fn restrict_current_user(&self, _path: &Path) -> PlatformResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManagedConfigKind {
    Claude,
    Codex,
    OpenCode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManagedConfigTargetKind {
    NativeWindows,
    CustomPath,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedConfigStatus {
    pub kind: ManagedConfigKind,
    pub target_kind: ManagedConfigTargetKind,
    pub config_path: String,
    pub backup_path: String,
    pub config_exists: bool,
    pub backup_exists: bool,
    pub managed: bool,
    pub uses_jsonc: bool,
    pub parse_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexActivationRequest {
    pub base_url: String,
    pub bearer_token: String,
    pub model: String,
    #[serde(default)]
    pub global_toml: String,
    #[serde(default)]
    pub node_toml: String,
    #[serde(default)]
    pub config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeActivationRequest {
    pub settings: ClaudeManagedSettings,
    #[serde(default)]
    pub config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeActivationRequest {
    pub node: OpenCodeManagedNode,
    #[serde(default)]
    pub common_settings: Option<Value>,
    #[serde(default)]
    pub config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUsageArchive {
    pub version: u32,
    pub updated_at_epoch_ms: u128,
    pub records: Vec<ProxyUsage>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUsageArchiveSummary {
    pub track: aiusage_core::ProxyTrack,
    pub path: String,
    pub records: usize,
    pub updated_at_epoch_ms: Option<u128>,
}

#[derive(Clone, Debug)]
pub struct ProxyUsageArchiveStore<P, G = NoopFilePermissionGuard> {
    paths: P,
    permissions: G,
}

impl<P> ProxyUsageArchiveStore<P, NoopFilePermissionGuard>
where
    P: AppPaths,
{
    pub fn new(paths: P) -> Self {
        Self {
            paths,
            permissions: NoopFilePermissionGuard,
        }
    }
}

impl<P, G> ProxyUsageArchiveStore<P, G>
where
    P: AppPaths,
    G: FilePermissionGuard,
{
    pub fn with_permissions(paths: P, permissions: G) -> Self {
        Self { paths, permissions }
    }

    pub fn append_usage(&self, usage: ProxyUsage) -> ServiceResult<ProxyUsageArchive> {
        let path = self.archive_path(&usage.track)?;
        let mut archive = self.load_archive_at(&path)?;
        archive.updated_at_epoch_ms = usage.observed_at_epoch_ms;
        archive.records.push(usage);
        self.write_archive(&path, &archive)?;
        Ok(archive)
    }

    pub fn summaries(&self) -> ServiceResult<Vec<ProxyUsageArchiveSummary>> {
        aiusage_proxy::all_proxy_tracks()
            .into_iter()
            .map(|track| {
                let path = self.archive_path(&track)?;
                let archive = self.load_archive_at(&path)?;
                Ok(ProxyUsageArchiveSummary {
                    track,
                    path: display_path(&path)?,
                    records: archive.records.len(),
                    updated_at_epoch_ms: (archive.updated_at_epoch_ms > 0)
                        .then_some(archive.updated_at_epoch_ms),
                })
            })
            .collect()
    }

    fn archive_path(&self, track: &aiusage_core::ProxyTrack) -> ServiceResult<PathBuf> {
        Ok(self
            .paths
            .app_config_dir()?
            .join("usage-archive")
            .join(format!(
                "proxy-usage-{}-v{PROXY_USAGE_ARCHIVE_VERSION}.json",
                archive_track_slug(track)
            )))
    }

    fn load_archive_at(&self, path: &Path) -> ServiceResult<ProxyUsageArchive> {
        match read_text_if_exists(path)? {
            Some(text) => Ok(serde_json::from_str(&text)?),
            None => Ok(ProxyUsageArchive {
                version: PROXY_USAGE_ARCHIVE_VERSION,
                updated_at_epoch_ms: 0,
                records: Vec::new(),
            }),
        }
    }

    fn write_archive(&self, path: &Path, archive: &ProxyUsageArchive) -> ServiceResult<()> {
        write_json_atomically(path, &serde_json::to_value(archive)?)?;
        self.permissions
            .restrict_current_user(path)
            .map_err(ServiceError::Platform)
    }
}

#[derive(Clone, Debug)]
pub struct ManagedConfigService<P, G = NoopFilePermissionGuard> {
    paths: P,
    permissions: G,
}

impl<P> ManagedConfigService<P, NoopFilePermissionGuard>
where
    P: AppPaths,
{
    pub fn new(paths: P) -> Self {
        Self {
            paths,
            permissions: NoopFilePermissionGuard,
        }
    }
}

impl<P, G> ManagedConfigService<P, G>
where
    P: AppPaths,
    G: FilePermissionGuard,
{
    pub fn with_permissions(paths: P, permissions: G) -> Self {
        Self { paths, permissions }
    }

    pub fn codex_status(&self, config_path: Option<PathBuf>) -> ServiceResult<ManagedConfigStatus> {
        let resolved = self.resolve_codex_target(config_path)?;
        codex_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn opencode_status(
        &self,
        config_path: Option<PathBuf>,
    ) -> ServiceResult<ManagedConfigStatus> {
        let resolved = self.resolve_opencode_target(config_path)?;
        opencode_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn statuses(&self) -> ServiceResult<Vec<ManagedConfigStatus>> {
        Ok(vec![
            self.claude_status(None)?,
            self.codex_status(None)?,
            self.opencode_status(None)?,
        ])
    }

    pub fn claude_status(
        &self,
        config_path: Option<PathBuf>,
    ) -> ServiceResult<ManagedConfigStatus> {
        let resolved = self.resolve_claude_target(config_path)?;
        claude_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn activate_claude(
        &self,
        request: ClaudeActivationRequest,
    ) -> ServiceResult<ManagedConfigStatus> {
        validate_claude_settings(&request.settings)?;
        let resolved = self.resolve_claude_target(request.config_path)?;
        let backup_path = backup_path_for(&resolved.path);
        let pristine = if backup_path.exists() {
            let backup = read_text_if_exists(&backup_path)?.unwrap_or_else(|| "{}".into());
            let root = parse_json_object(&backup, "Claude backup settings must be a JSON object")?;
            strip_claude_managed_settings(&root)
        } else if resolved.path.exists() {
            let current_text = read_text_if_exists(&resolved.path)?.unwrap_or_else(|| "{}".into());
            let current =
                parse_json_object(&current_text, "Claude settings.json must be a JSON object")?;
            self.copy_file_sensitive(&resolved.path, &backup_path)?;
            strip_claude_managed_settings(&current)
        } else {
            Value::Object(Default::default())
        };

        let next = inject_claude_managed_settings(&pristine, &request.settings);
        self.write_json_sensitive(&resolved.path, &next)?;
        claude_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn restore_claude(
        &self,
        config_path: Option<PathBuf>,
    ) -> ServiceResult<ManagedConfigStatus> {
        let resolved = self.resolve_claude_target(config_path)?;
        let backup_path = backup_path_for(&resolved.path);
        if backup_path.exists() {
            self.copy_file_sensitive(&backup_path, &resolved.path)?;
            fs::remove_file(&backup_path)?;
            return claude_status_for_path(resolved.path, resolved.target_kind);
        }

        let Some(current_text) = read_text_if_exists(&resolved.path)? else {
            return claude_status_for_path(resolved.path, resolved.target_kind);
        };
        let current =
            parse_json_object(&current_text, "Claude settings.json must be a JSON object")?;
        let clean = strip_claude_managed_settings(&current);
        if clean
            .as_object()
            .map(|object| object.is_empty())
            .unwrap_or(false)
        {
            remove_file_if_exists(&resolved.path)?;
        } else {
            self.write_json_sensitive(&resolved.path, &clean)?;
        }
        claude_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn activate_codex(
        &self,
        request: CodexActivationRequest,
    ) -> ServiceResult<ManagedConfigStatus> {
        if request.base_url.trim().is_empty() {
            return Err(ServiceError::InvalidRequest("Codex base_url is required"));
        }
        if request.model.trim().is_empty() {
            return Err(ServiceError::InvalidRequest("Codex model is required"));
        }

        let resolved = self.resolve_codex_target(request.config_path)?;
        let backup_path = backup_path_for(&resolved.path);
        let pristine = if backup_path.exists() {
            read_text_if_exists(&backup_path)?.unwrap_or_default()
        } else if resolved.path.exists() {
            let current = read_text_if_exists(&resolved.path)?.unwrap_or_default();
            let clean = if current.contains("AIUSAGE-CODEX") {
                strip_codex_managed_blocks(&current)
            } else {
                current
            };
            self.write_text_sensitive(&backup_path, &clean)?;
            clean
        } else {
            String::new()
        };

        let next = inject_codex_managed_config(
            &pristine,
            CodexManagedConfig {
                base_url: request.base_url.trim(),
                bearer_token: request.bearer_token.trim(),
                model: request.model.trim(),
                global_toml: &request.global_toml,
                node_toml: &request.node_toml,
            },
        );
        self.write_text_sensitive(&resolved.path, &next)?;
        codex_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn restore_codex(
        &self,
        config_path: Option<PathBuf>,
    ) -> ServiceResult<ManagedConfigStatus> {
        let resolved = self.resolve_codex_target(config_path)?;
        let backup_path = backup_path_for(&resolved.path);
        if backup_path.exists() {
            let backup = read_text_if_exists(&backup_path)?.unwrap_or_default();
            self.write_text_sensitive(&resolved.path, &backup)?;
            fs::remove_file(&backup_path)?;
            return codex_status_for_path(resolved.path, resolved.target_kind);
        }

        let Some(current) = read_text_if_exists(&resolved.path)? else {
            return codex_status_for_path(resolved.path, resolved.target_kind);
        };
        let clean = strip_codex_managed_blocks(&current);
        if clean.trim().is_empty() {
            remove_file_if_exists(&resolved.path)?;
        } else {
            self.write_text_sensitive(&resolved.path, &clean)?;
        }
        codex_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn activate_opencode(
        &self,
        request: OpenCodeActivationRequest,
    ) -> ServiceResult<ManagedConfigStatus> {
        validate_opencode_node(&request.node)?;
        let resolved = self.resolve_opencode_target(request.config_path)?;
        let backup_path = backup_path_for(&resolved.path);
        let pristine = if backup_path.exists() {
            let backup = read_text_if_exists(&backup_path)?.unwrap_or_else(|| "{}".into());
            strip_opencode_managed_entries(&parse_json_or_jsonc(&backup)?)
        } else if resolved.path.exists() {
            let current_text = read_text_if_exists(&resolved.path)?.unwrap_or_else(|| "{}".into());
            let current = parse_json_or_jsonc(&current_text)?;
            let clean = strip_opencode_managed_entries(&current);
            if opencode_has_managed_entries(&current) {
                self.write_json_sensitive(&backup_path, &clean)?;
            } else {
                self.copy_file_sensitive(&resolved.path, &backup_path)?;
            }
            clean
        } else {
            Value::Object(Default::default())
        };

        let next = inject_opencode_managed_config_with_base(
            &pristine,
            request.common_settings.as_ref(),
            &request.node,
        );
        self.write_json_sensitive(&resolved.path, &next)?;
        opencode_status_for_path(resolved.path, resolved.target_kind)
    }

    pub fn restore_opencode(
        &self,
        config_path: Option<PathBuf>,
    ) -> ServiceResult<ManagedConfigStatus> {
        let resolved = self.resolve_opencode_target(config_path)?;
        let backup_path = backup_path_for(&resolved.path);
        if backup_path.exists() {
            self.copy_file_sensitive(&backup_path, &resolved.path)?;
            fs::remove_file(&backup_path)?;
            return opencode_status_for_path(resolved.path, resolved.target_kind);
        }

        let Some(current_text) = read_text_if_exists(&resolved.path)? else {
            return opencode_status_for_path(resolved.path, resolved.target_kind);
        };
        let current = parse_json_or_jsonc(&current_text)?;
        let clean = strip_opencode_managed_entries(&current);
        if clean
            .as_object()
            .map(|object| object.keys().all(|key| key == "$schema"))
            .unwrap_or(false)
        {
            remove_file_if_exists(&resolved.path)?;
        } else {
            self.write_json_sensitive(&resolved.path, &clean)?;
        }
        opencode_status_for_path(resolved.path, resolved.target_kind)
    }

    fn resolve_codex_target(&self, config_path: Option<PathBuf>) -> ServiceResult<ResolvedTarget> {
        Ok(match config_path {
            Some(path) => ResolvedTarget {
                path,
                target_kind: ManagedConfigTargetKind::CustomPath,
            },
            None => ResolvedTarget {
                path: self.paths.codex_home()?.join("config.toml"),
                target_kind: ManagedConfigTargetKind::NativeWindows,
            },
        })
    }

    fn resolve_claude_target(&self, config_path: Option<PathBuf>) -> ServiceResult<ResolvedTarget> {
        Ok(match config_path {
            Some(path) => ResolvedTarget {
                path,
                target_kind: ManagedConfigTargetKind::CustomPath,
            },
            None => ResolvedTarget {
                path: self.paths.claude_home()?.join("settings.json"),
                target_kind: ManagedConfigTargetKind::NativeWindows,
            },
        })
    }

    fn resolve_opencode_target(
        &self,
        config_path: Option<PathBuf>,
    ) -> ServiceResult<ResolvedTarget> {
        Ok(match config_path {
            Some(path) => ResolvedTarget {
                path,
                target_kind: ManagedConfigTargetKind::CustomPath,
            },
            None => {
                let dir = self.paths.opencode_config_dir()?;
                let jsonc = dir.join("opencode.jsonc");
                let json = dir.join("opencode.json");
                ResolvedTarget {
                    path: if jsonc.exists() { jsonc } else { json },
                    target_kind: ManagedConfigTargetKind::NativeWindows,
                }
            }
        })
    }

    fn write_text_sensitive(&self, path: &Path, text: &str) -> ServiceResult<()> {
        write_text_atomically(path, text)?;
        self.restrict_sensitive_file(path)
    }

    fn write_json_sensitive(&self, path: &Path, value: &Value) -> ServiceResult<()> {
        write_json_atomically(path, value)?;
        self.restrict_sensitive_file(path)
    }

    fn copy_file_sensitive(&self, source: &Path, destination: &Path) -> ServiceResult<()> {
        copy_file_atomically(source, destination)?;
        self.restrict_sensitive_file(destination)
    }

    fn restrict_sensitive_file(&self, path: &Path) -> ServiceResult<()> {
        self.permissions
            .restrict_current_user(path)
            .map_err(ServiceError::Platform)
    }
}

#[derive(Clone, Debug)]
struct ResolvedTarget {
    path: PathBuf,
    target_kind: ManagedConfigTargetKind,
}

fn claude_status_for_path(
    path: PathBuf,
    target_kind: ManagedConfigTargetKind,
) -> ServiceResult<ManagedConfigStatus> {
    let content = read_text_if_exists(&path)?;
    let backup_path = backup_path_for(&path);
    let backup_exists = backup_path.exists();
    let (managed, parse_error) = match content.as_deref() {
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(root) => (backup_exists || claude_has_managed_entries(&root), None),
            Err(error) => (backup_exists, Some(error.to_string())),
        },
        None => (backup_exists, None),
    };

    Ok(ManagedConfigStatus {
        kind: ManagedConfigKind::Claude,
        target_kind,
        config_path: display_path(&path)?,
        backup_path: display_path(&backup_path)?,
        config_exists: content.is_some(),
        backup_exists,
        managed,
        uses_jsonc: false,
        parse_error,
    })
}

fn codex_status_for_path(
    path: PathBuf,
    target_kind: ManagedConfigTargetKind,
) -> ServiceResult<ManagedConfigStatus> {
    let content = read_text_if_exists(&path)?;
    let backup_path = backup_path_for(&path);
    let backup_exists = backup_path.exists();
    Ok(ManagedConfigStatus {
        kind: ManagedConfigKind::Codex,
        target_kind,
        config_path: display_path(&path)?,
        backup_path: display_path(&backup_path)?,
        config_exists: content.is_some(),
        backup_exists,
        managed: backup_exists
            || content
                .as_deref()
                .map(|content| content.contains("AIUSAGE-CODEX"))
                .unwrap_or(false),
        uses_jsonc: false,
        parse_error: None,
    })
}

fn opencode_status_for_path(
    path: PathBuf,
    target_kind: ManagedConfigTargetKind,
) -> ServiceResult<ManagedConfigStatus> {
    let content = read_text_if_exists(&path)?;
    let backup_path = backup_path_for(&path);
    let backup_exists = backup_path.exists();
    let (managed, parse_error) = match content.as_deref() {
        Some(text) => match parse_json_or_jsonc(text) {
            Ok(root) => (backup_exists || opencode_has_managed_entries(&root), None),
            Err(error) => (backup_exists, Some(error.to_string())),
        },
        None => (backup_exists, None),
    };

    Ok(ManagedConfigStatus {
        kind: ManagedConfigKind::OpenCode,
        target_kind,
        config_path: display_path(&path)?,
        backup_path: display_path(&backup_path)?,
        config_exists: content.is_some(),
        backup_exists,
        managed,
        uses_jsonc: path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("jsonc"))
            .unwrap_or(false),
        parse_error,
    })
}

fn validate_claude_settings(settings: &ClaudeManagedSettings) -> ServiceResult<()> {
    let has_any_value = [
        &settings.base_url,
        &settings.auth_token,
        &settings.default_model,
        &settings.opus_model,
        &settings.sonnet_model,
        &settings.haiku_model,
        &settings.node_extra_ca_certs,
    ]
    .into_iter()
    .any(|value| {
        value
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    });

    if has_any_value {
        Ok(())
    } else {
        Err(ServiceError::InvalidRequest(
            "Claude managed settings include no values",
        ))
    }
}

fn validate_opencode_node(node: &OpenCodeManagedNode) -> ServiceResult<()> {
    if node.managed_provider_id.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(
            "OpenCode managed_provider_id is required",
        ));
    }
    if node.base_url.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(
            "OpenCode base_url is required",
        ));
    }
    if node.default_model.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(
            "OpenCode default_model is required",
        ));
    }
    if node.models.is_empty() {
        return Err(ServiceError::InvalidRequest("OpenCode models are required"));
    }
    Ok(())
}

fn parse_json_object(text: &str, error: &'static str) -> ServiceResult<Value> {
    let value: Value = serde_json::from_str(text)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(ServiceError::InvalidRequest(error))
    }
}

fn read_text_if_exists(path: &Path) -> ServiceResult<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_text_atomically(path: &Path, text: &str) -> ServiceResult<()> {
    write_bytes_atomically(path, text.as_bytes())
}

fn write_json_atomically(path: &Path, value: &Value) -> ServiceResult<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_bytes_atomically(path, &bytes)
}

fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> ServiceResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = temp_path_for(path);
    fs::write(&temp_path, bytes)?;
    replace_file(&temp_path, path)?;
    Ok(())
}

fn copy_file_atomically(source: &Path, destination: &Path) -> ServiceResult<()> {
    let bytes = fs::read(source)?;
    write_bytes_atomically(destination, &bytes)
}

fn replace_file(source: &Path, destination: &Path) -> ServiceResult<()> {
    remove_file_if_exists(destination)?;
    fs::rename(source, destination)?;
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> ServiceResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn backup_path_for(path: &Path) -> PathBuf {
    let mut raw = OsString::from(path.as_os_str());
    raw.push(".aiusage.bak");
    PathBuf::from(raw)
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut raw = OsString::from(path.as_os_str());
    raw.push(".aiusage.tmp");
    PathBuf::from(raw)
}

fn display_path(path: &Path) -> ServiceResult<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ServiceError::NonUtf8Path(path.to_path_buf()))
}

fn archive_track_slug(track: &aiusage_core::ProxyTrack) -> &'static str {
    match track {
        aiusage_core::ProxyTrack::ClaudeCode => "claude",
        aiusage_core::ProxyTrack::Codex => "codex",
        aiusage_core::ProxyTrack::OpenCode => "opencode",
        aiusage_core::ProxyTrack::Global => "global",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiusage_core::OpenCodeManagedModel;
    use aiusage_platform::PlatformResult;
    use aiusage_proxy::{ProxyProtocol, ProxyUsage};
    use serde_json::json;
    use tempfile::TempDir;

    #[derive(Clone, Debug)]
    struct TestPaths {
        root: PathBuf,
    }

    impl TestPaths {
        fn new(root: PathBuf) -> Self {
            Self { root }
        }
    }

    impl AppPaths for TestPaths {
        fn app_config_dir(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.join("appdata").join("AIUsage"))
        }

        fn app_data_dir(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.join("localappdata").join("AIUsage"))
        }

        fn app_cache_dir(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.join("localappdata").join("AIUsage").join("cache"))
        }

        fn codex_home(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.join(".codex"))
        }

        fn claude_home(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.join(".claude"))
        }

        fn opencode_config_dir(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.join(".config").join("opencode"))
        }
    }

    #[test]
    fn codex_activation_is_idempotent_and_restores_original() {
        let temp = TempDir::new().expect("tempdir");
        let service = ManagedConfigService::new(TestPaths::new(temp.path().to_path_buf()));
        let codex_dir = temp.path().join(".codex");
        fs::create_dir_all(&codex_dir).expect("codex dir");
        let config_path = codex_dir.join("config.toml");
        fs::write(
            &config_path,
            "model = \"old\"\nmodel_reasoning_effort = \"low\"\n",
        )
        .expect("seed config");

        let request = CodexActivationRequest {
            base_url: "http://127.0.0.1:4317/v1".into(),
            bearer_token: "client-key".into(),
            model: "gpt-5".into(),
            global_toml: "model_reasoning_effort = \"medium\"".into(),
            node_toml: "model_reasoning_effort = \"high\"".into(),
            config_path: None,
        };
        let first = service.activate_codex(request.clone()).expect("activate");
        let second = service.activate_codex(request).expect("reactivate");
        assert!(first.managed);
        assert!(second.backup_exists);
        assert_eq!(
            fs::read_to_string(backup_path_for(&config_path)).expect("backup"),
            "model = \"old\"\nmodel_reasoning_effort = \"low\"\n"
        );

        service.restore_codex(None).expect("restore");
        assert_eq!(
            fs::read_to_string(config_path).expect("restored"),
            "model = \"old\"\nmodel_reasoning_effort = \"low\"\n"
        );
    }

    #[test]
    fn codex_restore_removes_managed_only_file_without_backup() {
        let temp = TempDir::new().expect("tempdir");
        let service = ManagedConfigService::new(TestPaths::new(temp.path().to_path_buf()));
        let status = service
            .activate_codex(CodexActivationRequest {
                base_url: "http://127.0.0.1:4317/v1".into(),
                bearer_token: "client-key".into(),
                model: "gpt-5".into(),
                global_toml: String::new(),
                node_toml: String::new(),
                config_path: None,
            })
            .expect("activate");
        assert!(status.managed);
        assert!(!status.backup_exists);

        let restored = service.restore_codex(None).expect("restore");
        assert!(!restored.config_exists);
    }

    #[test]
    fn opencode_activation_preserves_jsonc_backup_and_restores_verbatim() {
        let temp = TempDir::new().expect("tempdir");
        let service = ManagedConfigService::new(TestPaths::new(temp.path().to_path_buf()));
        let dir = temp.path().join(".config").join("opencode");
        fs::create_dir_all(&dir).expect("opencode dir");
        let config_path = dir.join("opencode.jsonc");
        let original = "{\n  // keep me\n  \"theme\": \"system\",\n}\n";
        fs::write(&config_path, original).expect("seed jsonc");

        let request = OpenCodeActivationRequest {
            node: node(),
            common_settings: Some(json!({"theme": "dark", "provider": {"aiusage-old": {}}})),
            config_path: None,
        };
        let status = service.activate_opencode(request).expect("activate");
        assert!(status.managed);
        assert!(status.uses_jsonc);
        assert_eq!(
            fs::read_to_string(backup_path_for(&config_path)).expect("backup"),
            original
        );
        let managed = fs::read_to_string(&config_path).expect("managed config");
        assert!(managed.contains("\"aiusage-main\""));
        assert!(managed.contains("\"theme\": \"dark\""));
        assert!(!managed.contains("aiusage-old"));

        service.restore_opencode(None).expect("restore");
        assert_eq!(fs::read_to_string(config_path).expect("restored"), original);
    }

    #[test]
    fn opencode_status_reports_parse_errors_without_throwing() {
        let temp = TempDir::new().expect("tempdir");
        let service = ManagedConfigService::new(TestPaths::new(temp.path().to_path_buf()));
        let dir = temp.path().join(".config").join("opencode");
        fs::create_dir_all(&dir).expect("opencode dir");
        fs::write(dir.join("opencode.json"), "{ broken").expect("seed invalid config");

        let status = service.opencode_status(None).expect("status");
        assert!(status.parse_error.is_some());
        assert!(!status.managed);
    }

    #[test]
    fn proxy_usage_archive_appends_per_track_records() {
        let temp = TempDir::new().expect("tempdir");
        let store = ProxyUsageArchiveStore::new(TestPaths::new(temp.path().to_path_buf()));
        let archive = store
            .append_usage(ProxyUsage {
                track: aiusage_core::ProxyTrack::Codex,
                node_id: "node-1".into(),
                protocol: ProxyProtocol::OpenAiResponses,
                model: Some("gpt-5".into()),
                input_tokens: 10,
                output_tokens: 4,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                observed_at_epoch_ms: 42,
            })
            .expect("append usage");
        assert_eq!(archive.records.len(), 1);

        let summary = store
            .summaries()
            .expect("summaries")
            .into_iter()
            .find(|summary| summary.track == aiusage_core::ProxyTrack::Codex)
            .expect("codex summary");
        assert_eq!(summary.records, 1);
        assert!(summary.path.ends_with("proxy-usage-codex-v1.json"));
    }

    #[test]
    fn claude_activation_restores_original_settings_verbatim() {
        let temp = TempDir::new().expect("tempdir");
        let service = ManagedConfigService::new(TestPaths::new(temp.path().to_path_buf()));
        let dir = temp.path().join(".claude");
        fs::create_dir_all(&dir).expect("claude dir");
        let config_path = dir.join("settings.json");
        let original = "{\n  \"env\": {\"PATH\": \"keep\"},\n  \"model\": \"user-model\"\n}\n";
        fs::write(&config_path, original).expect("seed settings");

        let status = service
            .activate_claude(ClaudeActivationRequest {
                settings: ClaudeManagedSettings {
                    base_url: Some("http://127.0.0.1:4315".into()),
                    auth_token: Some("client-key".into()),
                    default_model: Some("claude-sonnet-4".into()),
                    ..Default::default()
                },
                config_path: None,
            })
            .expect("activate");
        assert!(status.managed);
        assert!(status.backup_exists);

        let managed = fs::read_to_string(&config_path).expect("managed settings");
        assert!(managed.contains("ANTHROPIC_BASE_URL"));
        assert!(managed.contains("client-key"));

        service.restore_claude(None).expect("restore");
        assert_eq!(fs::read_to_string(config_path).expect("restored"), original);
    }

    #[test]
    fn claude_restore_without_backup_strips_managed_values() {
        let temp = TempDir::new().expect("tempdir");
        let service = ManagedConfigService::new(TestPaths::new(temp.path().to_path_buf()));
        let status = service
            .activate_claude(ClaudeActivationRequest {
                settings: ClaudeManagedSettings {
                    base_url: Some("http://127.0.0.1:4315".into()),
                    auth_token: Some("client-key".into()),
                    default_model: Some("claude-sonnet-4".into()),
                    ..Default::default()
                },
                config_path: None,
            })
            .expect("activate");
        assert!(status.managed);
        assert!(!status.backup_exists);

        let restored = service.restore_claude(None).expect("restore");
        assert!(!restored.config_exists);
    }

    fn node() -> OpenCodeManagedNode {
        OpenCodeManagedNode {
            managed_provider_id: "aiusage-main".into(),
            display_name: "AIUsage Main".into(),
            npm_package: "@ai-sdk/openai-compatible".into(),
            base_url: "http://127.0.0.1:4321/v1".into(),
            api_key: Some("client-key".into()),
            default_model: "gpt-5".into(),
            models: vec![OpenCodeManagedModel {
                id: "gpt-5".into(),
                display_name: None,
            }],
        }
    }
}
