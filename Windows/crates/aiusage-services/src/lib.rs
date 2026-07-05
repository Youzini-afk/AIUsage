use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use aiusage_core::{
    claude_has_managed_entries, inject_claude_managed_settings, inject_codex_managed_config,
    inject_opencode_managed_config_with_base, opencode_has_managed_entries, parse_json_or_jsonc,
    strip_claude_managed_settings, strip_codex_managed_blocks, strip_opencode_managed_entries,
    ClaudeManagedSettings, CodexManagedConfig, CredentialKind, OpenCodeManagedNode,
};
use aiusage_platform::{
    AppPaths, CredentialVault, FilePermissionGuard, PlatformError, PlatformResult,
};
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenTotals {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }

    fn add_usage(&mut self, usage: &ProxyUsage) {
        self.requests = self.requests.saturating_add(1);
        self.input_tokens = self.input_tokens.saturating_add(usage.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(usage.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(usage.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(usage.cache_write_tokens);
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUsageModelBreakdown {
    pub track: aiusage_core::ProxyTrack,
    pub model: String,
    pub totals: TokenTotals,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUsageTrackBreakdown {
    pub track: aiusage_core::ProxyTrack,
    pub totals: TokenTotals,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUsageStats {
    pub totals: TokenTotals,
    pub by_track: Vec<ProxyUsageTrackBreakdown>,
    pub by_model: Vec<ProxyUsageModelBreakdown>,
    pub updated_at_epoch_ms: Option<u128>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CallAnalyticsSource {
    Claude,
    Codex,
    OpenCode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsPathStatus {
    pub path: String,
    pub exists: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsInventorySourceStatus {
    pub source: CallAnalyticsSource,
    pub available: bool,
    pub config_paths: Vec<CallAnalyticsPathStatus>,
    pub session_paths: Vec<CallAnalyticsPathStatus>,
    pub skill_paths: Vec<CallAnalyticsPathStatus>,
    pub config_file_count: usize,
    pub session_file_count: usize,
    pub skill_count: usize,
    pub mcp_server_count: usize,
    pub skill_names: Vec<String>,
    pub mcp_server_names: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsInventorySnapshot {
    pub generated_at_epoch_ms: u128,
    pub sources: Vec<CallAnalyticsInventorySourceStatus>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialVaultDocument {
    pub version: u32,
    pub credentials: Vec<StoredCredential>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCredential {
    pub id: String,
    pub provider_id: String,
    pub label: String,
    pub kind: CredentialKind,
    pub secret: String,
    pub metadata: Value,
    pub created_at_epoch_ms: u128,
    pub updated_at_epoch_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSummary {
    pub id: String,
    pub provider_id: String,
    pub label: String,
    pub kind: CredentialKind,
    pub has_secret: bool,
    pub metadata: Value,
    pub updated_at_epoch_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertCredentialRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub label: String,
    pub kind: CredentialKind,
    pub secret: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug)]
pub struct CredentialRegistry<V> {
    vault: V,
}

impl<V> CredentialRegistry<V>
where
    V: CredentialVault,
{
    pub fn new(vault: V) -> Self {
        Self { vault }
    }

    pub fn list_summaries(&self) -> ServiceResult<Vec<CredentialSummary>> {
        Ok(self
            .load_document()?
            .credentials
            .into_iter()
            .map(|credential| CredentialSummary {
                id: credential.id,
                provider_id: credential.provider_id,
                label: credential.label,
                kind: credential.kind,
                has_secret: !credential.secret.is_empty(),
                metadata: credential.metadata,
                updated_at_epoch_ms: credential.updated_at_epoch_ms,
            })
            .collect())
    }

    pub fn upsert(&self, request: UpsertCredentialRequest) -> ServiceResult<CredentialSummary> {
        validate_credential_request(&request)?;
        let mut document = self.load_document()?;
        let now = epoch_ms();
        let id = request.id.unwrap_or_else(|| format!("cred-{now}"));

        let mut created_at = now;
        document.credentials.retain(|credential| {
            if credential.id == id {
                created_at = credential.created_at_epoch_ms;
                false
            } else {
                true
            }
        });

        let credential = StoredCredential {
            id: id.clone(),
            provider_id: request.provider_id.trim().to_string(),
            label: request.label.trim().to_string(),
            kind: request.kind,
            secret: request.secret,
            metadata: request.metadata,
            created_at_epoch_ms: created_at,
            updated_at_epoch_ms: now,
        };
        let summary = CredentialSummary {
            id,
            provider_id: credential.provider_id.clone(),
            label: credential.label.clone(),
            kind: credential.kind.clone(),
            has_secret: !credential.secret.is_empty(),
            metadata: credential.metadata.clone(),
            updated_at_epoch_ms: credential.updated_at_epoch_ms,
        };
        document.credentials.push(credential);
        self.save_document(&document)?;
        Ok(summary)
    }

    pub fn delete(&self, id: &str) -> ServiceResult<bool> {
        let mut document = self.load_document()?;
        let before = document.credentials.len();
        document
            .credentials
            .retain(|credential| credential.id != id);
        let removed = before != document.credentials.len();
        if removed {
            if document.credentials.is_empty() {
                self.vault.delete_vault()?;
            } else {
                self.save_document(&document)?;
            }
        }
        Ok(removed)
    }

    pub fn reveal(&self, id: &str) -> ServiceResult<Option<StoredCredential>> {
        Ok(self
            .load_document()?
            .credentials
            .into_iter()
            .find(|credential| credential.id == id))
    }

    fn load_document(&self) -> ServiceResult<CredentialVaultDocument> {
        let Some(data) = self.vault.load_vault()? else {
            return Ok(CredentialVaultDocument {
                version: 1,
                credentials: Vec::new(),
            });
        };
        Ok(serde_json::from_slice(&data)?)
    }

    fn save_document(&self, document: &CredentialVaultDocument) -> ServiceResult<()> {
        self.vault
            .save_vault(&serde_json::to_vec_pretty(document)?)?;
        Ok(())
    }
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

    pub fn usage_stats(&self) -> ServiceResult<ProxyUsageStats> {
        let mut totals = TokenTotals::default();
        let mut by_track =
            std::collections::BTreeMap::<String, (aiusage_core::ProxyTrack, TokenTotals)>::new();
        let mut by_model = std::collections::BTreeMap::<
            (String, String),
            (aiusage_core::ProxyTrack, String, TokenTotals),
        >::new();
        let mut updated_at_epoch_ms = None;

        for track in aiusage_proxy::all_proxy_tracks() {
            let path = self.archive_path(&track)?;
            let archive = self.load_archive_at(&path)?;
            if archive.updated_at_epoch_ms > 0 {
                updated_at_epoch_ms = Some(
                    updated_at_epoch_ms
                        .unwrap_or(0)
                        .max(archive.updated_at_epoch_ms),
                );
            }

            for usage in archive.records {
                totals.add_usage(&usage);
                let track_key = archive_track_slug(&usage.track).to_string();
                by_track
                    .entry(track_key)
                    .or_insert_with(|| (usage.track.clone(), TokenTotals::default()))
                    .1
                    .add_usage(&usage);

                let model = usage.model.as_deref().unwrap_or("unknown").to_string();
                let model_key = (archive_track_slug(&usage.track).to_string(), model.clone());
                by_model
                    .entry(model_key)
                    .or_insert_with(|| (usage.track.clone(), model, TokenTotals::default()))
                    .2
                    .add_usage(&usage);
            }
        }

        Ok(ProxyUsageStats {
            totals,
            by_track: by_track
                .into_values()
                .map(|(track, totals)| ProxyUsageTrackBreakdown { track, totals })
                .collect(),
            by_model: by_model
                .into_values()
                .map(|(track, model, totals)| ProxyUsageModelBreakdown {
                    track,
                    model,
                    totals,
                })
                .collect(),
            updated_at_epoch_ms,
        })
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
pub struct CallAnalyticsInventoryService<P> {
    paths: P,
}

impl<P> CallAnalyticsInventoryService<P>
where
    P: AppPaths,
{
    pub fn new(paths: P) -> Self {
        Self { paths }
    }

    pub fn snapshot(&self) -> ServiceResult<CallAnalyticsInventorySnapshot> {
        Ok(CallAnalyticsInventorySnapshot {
            generated_at_epoch_ms: epoch_ms(),
            sources: vec![
                self.claude_inventory()?,
                self.codex_inventory()?,
                self.opencode_inventory()?,
            ],
        })
    }

    fn claude_inventory(&self) -> ServiceResult<CallAnalyticsInventorySourceStatus> {
        let home = self.paths.user_home()?;
        let claude_home = self.paths.claude_home()?;
        let configs = vec![
            (home.join(".claude.json"), McpConfigFormat::Json),
            (claude_home.join("settings.json"), McpConfigFormat::Json),
        ];
        let skill_roots = vec![claude_home.join("skills")];
        let session_roots = vec![claude_home.join("projects")];
        self.inventory_for_source(
            CallAnalyticsSource::Claude,
            configs,
            session_roots,
            SessionProbeKind::JsonLines,
            skill_roots,
        )
    }

    fn codex_inventory(&self) -> ServiceResult<CallAnalyticsInventorySourceStatus> {
        let codex_home = self.paths.codex_home()?;
        let configs = vec![(codex_home.join("config.toml"), McpConfigFormat::CodexToml)];
        let skill_roots = vec![codex_home.join("skills")];
        let session_roots = vec![
            codex_home.join("sessions"),
            codex_home.join("archived_sessions"),
        ];
        self.inventory_for_source(
            CallAnalyticsSource::Codex,
            configs,
            session_roots,
            SessionProbeKind::JsonLines,
            skill_roots,
        )
    }

    fn opencode_inventory(&self) -> ServiceResult<CallAnalyticsInventorySourceStatus> {
        let config_dir = self.paths.opencode_config_dir()?;
        let configs = vec![
            (config_dir.join("opencode.json"), McpConfigFormat::Json),
            (config_dir.join("opencode.jsonc"), McpConfigFormat::Json),
        ];
        let skill_roots = vec![config_dir.join("skills")];
        let session_paths = self
            .opencode_data_dirs()?
            .into_iter()
            .map(|dir| dir.join("opencode.db"))
            .collect();
        self.inventory_for_source(
            CallAnalyticsSource::OpenCode,
            configs,
            session_paths,
            SessionProbeKind::ExactFile,
            skill_roots,
        )
    }

    fn inventory_for_source(
        &self,
        source: CallAnalyticsSource,
        configs: Vec<(PathBuf, McpConfigFormat)>,
        session_paths: Vec<PathBuf>,
        session_kind: SessionProbeKind,
        skill_roots: Vec<PathBuf>,
    ) -> ServiceResult<CallAnalyticsInventorySourceStatus> {
        let mut warnings = Vec::new();
        let mut skill_names = BTreeSet::new();
        for root in &skill_roots {
            collect_skill_names(root, &mut skill_names, &mut warnings);
        }

        let mut mcp_server_names = BTreeSet::new();
        for (path, format) in &configs {
            collect_mcp_names(path, *format, &mut mcp_server_names, &mut warnings);
        }

        let config_paths = path_statuses(configs.iter().map(|(path, _)| path))?;
        let session_path_statuses = path_statuses(session_paths.iter())?;
        let skill_paths = path_statuses(skill_roots.iter())?;
        let config_file_count = config_paths.iter().filter(|status| status.exists).count();
        let session_file_count = count_session_files(&session_paths, session_kind, &mut warnings);

        Ok(CallAnalyticsInventorySourceStatus {
            source,
            available: config_file_count > 0 || session_file_count > 0 || !skill_names.is_empty(),
            config_paths,
            session_paths: session_path_statuses,
            skill_paths,
            config_file_count,
            session_file_count,
            skill_count: skill_names.len(),
            mcp_server_count: mcp_server_names.len(),
            skill_names: skill_names.into_iter().collect(),
            mcp_server_names: mcp_server_names.into_iter().collect(),
            warnings,
        })
    }

    fn opencode_data_dirs(&self) -> ServiceResult<Vec<PathBuf>> {
        let mut dirs = Vec::new();
        if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME") {
            push_unique_path(&mut dirs, PathBuf::from(xdg_data_home).join("opencode"));
        }
        if let Some(local_data_root) = self.paths.app_data_dir()?.parent().map(Path::to_path_buf) {
            push_unique_path(&mut dirs, local_data_root.join("opencode"));
        }
        push_unique_path(
            &mut dirs,
            self.paths
                .user_home()?
                .join(".local")
                .join("share")
                .join("opencode"),
        );
        push_unique_path(&mut dirs, self.paths.opencode_config_dir()?);
        Ok(dirs)
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

#[derive(Clone, Copy, Debug)]
enum McpConfigFormat {
    Json,
    CodexToml,
}

#[derive(Clone, Copy, Debug)]
enum SessionProbeKind {
    JsonLines,
    ExactFile,
}

const SKILL_MARKER_FILENAME: &str = "SKILL.md";
const SKIP_SCAN_DIRECTORIES: &[&str] = &["node_modules", ".git", "Pods", "dist", "build", "target"];

fn collect_skill_names(root: &Path, names: &mut BTreeSet<String>, warnings: &mut Vec<String>) {
    let Ok(metadata) = fs::metadata(root) else {
        return;
    };
    if !metadata.is_dir() {
        return;
    }
    collect_skill_names_in_dir(root, names, warnings);
}

fn collect_skill_names_in_dir(
    directory: &Path,
    names: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("{}: {}", directory.display(), error));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("{}: {}", directory.display(), error));
                continue;
            }
        };
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if SKIP_SCAN_DIRECTORIES.contains(&file_name.as_str()) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                warnings.push(format!("{}: {}", path.display(), error));
                continue;
            }
        };

        if file_type.is_dir() {
            collect_skill_names_in_dir(&path, names, warnings);
        } else if file_type.is_file() && file_name == SKILL_MARKER_FILENAME {
            if let Some(name) = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                names.insert(name.to_string());
            }
        }
    }
}

fn collect_mcp_names(
    path: &Path,
    format: McpConfigFormat,
    names: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) {
    let text = match read_text_if_exists(path) {
        Ok(Some(text)) => text,
        Ok(None) => return,
        Err(error) => {
            warnings.push(format!("{}: {}", path.display(), error));
            return;
        }
    };

    match format {
        McpConfigFormat::Json => match parse_json_or_jsonc(&text) {
            Ok(value) => collect_json_mcp_names(&value, names),
            Err(error) => warnings.push(format!("{}: {}", path.display(), error)),
        },
        McpConfigFormat::CodexToml => collect_codex_toml_mcp_names(&text, names),
    }
}

fn collect_json_mcp_names(value: &Value, names: &mut BTreeSet<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    add_json_mcp_server_keys(object, names);

    if let Some(projects) = object.get("projects").and_then(Value::as_object) {
        for project in projects.values().filter_map(Value::as_object) {
            add_json_mcp_server_keys(project, names);
        }
    }
}

fn add_json_mcp_server_keys(object: &serde_json::Map<String, Value>, names: &mut BTreeSet<String>) {
    for key in ["mcpServers", "mcp", "mcp_servers"] {
        if let Some(servers) = object.get(key).and_then(Value::as_object) {
            for name in servers.keys().map(String::as_str).map(str::trim) {
                if !name.is_empty() {
                    names.insert(name.to_string());
                }
            }
        }
    }
}

fn collect_codex_toml_mcp_names(text: &str, names: &mut BTreeSet<String>) {
    let prefix = "mcp_servers.";
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if !line.starts_with('[') || line.starts_with("[[") {
            continue;
        }
        let Some(close) = line.find(']') else {
            continue;
        };
        let inner = &line[1..close];
        let Some(rest) = inner.strip_prefix(prefix) else {
            continue;
        };
        if let Some(name) = first_toml_key_segment(rest) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                names.insert(trimmed.to_string());
            }
        }
    }
}

fn first_toml_key_segment(raw: &str) -> Option<String> {
    let raw = raw.trim_start();
    let mut chars = raw.chars();
    match chars.next()? {
        quote @ ('\'' | '"') => {
            let rest = &raw[quote.len_utf8()..];
            rest.find(quote).map(|end| rest[..end].to_string())
        }
        _ => raw
            .split('.')
            .next()
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn path_statuses<'a>(
    paths: impl IntoIterator<Item = &'a PathBuf>,
) -> ServiceResult<Vec<CallAnalyticsPathStatus>> {
    paths
        .into_iter()
        .map(|path| {
            Ok(CallAnalyticsPathStatus {
                path: display_path(path)?,
                exists: path.exists(),
            })
        })
        .collect()
}

fn count_session_files(
    paths: &[PathBuf],
    kind: SessionProbeKind,
    warnings: &mut Vec<String>,
) -> usize {
    match kind {
        SessionProbeKind::JsonLines => paths
            .iter()
            .map(|path| count_matching_files(path, &["jsonl", "json"], warnings))
            .sum(),
        SessionProbeKind::ExactFile => paths.iter().filter(|path| path.is_file()).count(),
    }
}

fn count_matching_files(root: &Path, extensions: &[&str], warnings: &mut Vec<String>) -> usize {
    let Ok(metadata) = fs::metadata(root) else {
        return 0;
    };
    if metadata.is_file() {
        return file_extension_matches(root, extensions) as usize;
    }
    if !metadata.is_dir() {
        return 0;
    }

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("{}: {}", root.display(), error));
            return 0;
        }
    };

    let mut count = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("{}: {}", root.display(), error));
                continue;
            }
        };
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if SKIP_SCAN_DIRECTORIES.contains(&file_name.as_str()) {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                warnings.push(format!("{}: {}", path.display(), error));
                continue;
            }
        };

        if file_type.is_dir() {
            count += count_matching_files(&path, extensions, warnings);
        } else if file_type.is_file() && file_extension_matches(&path, extensions) {
            count += 1;
        }
    }
    count
}

fn file_extension_matches(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extensions
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or(false)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn validate_credential_request(request: &UpsertCredentialRequest) -> ServiceResult<()> {
    if request.provider_id.trim().is_empty() {
        return Err(ServiceError::InvalidRequest(
            "credential provider_id is required",
        ));
    }
    if request.label.trim().is_empty() {
        return Err(ServiceError::InvalidRequest("credential label is required"));
    }
    if request.secret.is_empty() {
        return Err(ServiceError::InvalidRequest(
            "credential secret is required",
        ));
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

fn epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiusage_core::OpenCodeManagedModel;
    use aiusage_platform::{CredentialVault, PlatformResult};
    use aiusage_proxy::{ProxyProtocol, ProxyUsage};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
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
        fn user_home(&self) -> PlatformResult<PathBuf> {
            Ok(self.root.clone())
        }

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

    #[derive(Clone, Debug, Default)]
    struct MemoryVault {
        data: Arc<Mutex<Option<Vec<u8>>>>,
    }

    impl CredentialVault for MemoryVault {
        fn load_vault(&self) -> PlatformResult<Option<Vec<u8>>> {
            Ok(self.data.lock().expect("vault lock").clone())
        }

        fn save_vault(&self, data: &[u8]) -> PlatformResult<()> {
            *self.data.lock().expect("vault lock") = Some(data.to_vec());
            Ok(())
        }

        fn delete_vault(&self) -> PlatformResult<()> {
            *self.data.lock().expect("vault lock") = None;
            Ok(())
        }

        fn supported_kinds(&self) -> Vec<CredentialKind> {
            vec![CredentialKind::ApiKey, CredentialKind::Token]
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
    fn proxy_usage_stats_aggregate_totals_tracks_and_models() {
        let temp = TempDir::new().expect("tempdir");
        let store = ProxyUsageArchiveStore::new(TestPaths::new(temp.path().to_path_buf()));
        store
            .append_usage(ProxyUsage {
                track: aiusage_core::ProxyTrack::Codex,
                node_id: "node-1".into(),
                protocol: ProxyProtocol::OpenAiResponses,
                model: Some("gpt-5".into()),
                input_tokens: 10,
                output_tokens: 4,
                cache_read_tokens: 1,
                cache_write_tokens: 0,
                observed_at_epoch_ms: 42,
            })
            .expect("append usage");
        store
            .append_usage(ProxyUsage {
                track: aiusage_core::ProxyTrack::ClaudeCode,
                node_id: "node-2".into(),
                protocol: ProxyProtocol::AnthropicMessages,
                model: Some("claude-sonnet-4".into()),
                input_tokens: 20,
                output_tokens: 5,
                cache_read_tokens: 0,
                cache_write_tokens: 2,
                observed_at_epoch_ms: 84,
            })
            .expect("append usage");

        let stats = store.usage_stats().expect("stats");
        assert_eq!(stats.totals.requests, 2);
        assert_eq!(stats.totals.input_tokens, 30);
        assert_eq!(stats.totals.output_tokens, 9);
        assert_eq!(stats.totals.cache_read_tokens, 1);
        assert_eq!(stats.totals.cache_write_tokens, 2);
        assert_eq!(stats.updated_at_epoch_ms, Some(84));
        assert_eq!(stats.by_track.len(), 2);
        assert_eq!(stats.by_model.len(), 2);
        assert!(stats.by_model.iter().any(|row| row.model == "gpt-5"));
    }

    #[test]
    fn call_analytics_inventory_scans_cli_configs_skills_and_sessions() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();

        let claude_skill = root.join(".claude").join("skills").join("briefing");
        fs::create_dir_all(&claude_skill).expect("claude skill dir");
        fs::write(claude_skill.join("SKILL.md"), "# briefing").expect("claude skill");
        let claude_project = root.join(".claude").join("projects").join("workspace");
        fs::create_dir_all(&claude_project).expect("claude project dir");
        fs::write(claude_project.join("session.jsonl"), "{}\n").expect("claude session");
        fs::write(
            root.join(".claude.json"),
            r#"{"projects":{"E:\\repo":{"mcpServers":{"fs":{},"git":{}}}}}"#,
        )
        .expect("claude config");

        let codex_home = root.join(".codex");
        fs::create_dir_all(codex_home.join("skills").join("review")).expect("codex skill dir");
        fs::write(
            codex_home.join("skills").join("review").join("SKILL.md"),
            "# review",
        )
        .expect("codex skill");
        fs::create_dir_all(codex_home.join("sessions").join("2026")).expect("codex sessions");
        fs::create_dir_all(codex_home.join("archived_sessions")).expect("codex archived sessions");
        fs::write(
            codex_home.join("sessions").join("2026").join("one.jsonl"),
            "{}\n",
        )
        .expect("codex session");
        fs::write(codex_home.join("archived_sessions").join("two.json"), "{}")
            .expect("codex archived session");
        fs::write(
            codex_home.join("config.toml"),
            "[mcp_servers.demo]\ncommand = \"node\"\n[mcp_servers.\"quoted.name\".env]\nA = \"B\"\n",
        )
        .expect("codex config");

        let opencode_config = root.join(".config").join("opencode");
        fs::create_dir_all(opencode_config.join("skills").join("plan")).expect("opencode skill");
        fs::write(
            opencode_config.join("skills").join("plan").join("SKILL.md"),
            "# plan",
        )
        .expect("opencode skill");
        fs::write(
            opencode_config.join("opencode.jsonc"),
            "{ // comment\n \"mcp\": {\"context7\": {},},\n}\n",
        )
        .expect("opencode config");
        let opencode_data = root.join("localappdata").join("opencode");
        fs::create_dir_all(&opencode_data).expect("opencode data dir");
        fs::write(opencode_data.join("opencode.db"), "").expect("opencode db");

        let service = CallAnalyticsInventoryService::new(TestPaths::new(root.to_path_buf()));
        let snapshot = service.snapshot().expect("inventory snapshot");
        assert_eq!(snapshot.sources.len(), 3);

        let claude = inventory_source(&snapshot, CallAnalyticsSource::Claude);
        assert!(claude.available);
        assert_eq!(claude.session_file_count, 1);
        assert_eq!(claude.skill_names, vec!["briefing"]);
        assert_eq!(claude.mcp_server_names, vec!["fs", "git"]);

        let codex = inventory_source(&snapshot, CallAnalyticsSource::Codex);
        assert_eq!(codex.session_file_count, 2);
        assert_eq!(codex.skill_names, vec!["review"]);
        assert_eq!(codex.mcp_server_names, vec!["demo", "quoted.name"]);

        let opencode = inventory_source(&snapshot, CallAnalyticsSource::OpenCode);
        assert_eq!(opencode.session_file_count, 1);
        assert_eq!(opencode.skill_names, vec!["plan"]);
        assert_eq!(opencode.mcp_server_names, vec!["context7"]);
        assert!(opencode.warnings.is_empty());
    }

    #[test]
    fn call_analytics_inventory_reports_parse_warnings_without_failing() {
        let temp = TempDir::new().expect("tempdir");
        let config_dir = temp.path().join(".config").join("opencode");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::write(config_dir.join("opencode.json"), "{ broken").expect("broken config");

        let service = CallAnalyticsInventoryService::new(TestPaths::new(temp.path().to_path_buf()));
        let snapshot = service.snapshot().expect("inventory snapshot");
        let opencode = inventory_source(&snapshot, CallAnalyticsSource::OpenCode);
        assert!(opencode.available);
        assert_eq!(opencode.config_file_count, 1);
        assert_eq!(opencode.mcp_server_count, 0);
        assert_eq!(opencode.warnings.len(), 1);
    }

    #[test]
    fn credential_registry_stores_summaries_without_exposing_secret() {
        let registry = CredentialRegistry::new(MemoryVault::default());
        let summary = registry
            .upsert(UpsertCredentialRequest {
                id: Some("cred-1".into()),
                provider_id: "codex".into(),
                label: "Codex Auth".into(),
                kind: CredentialKind::Token,
                secret: "secret-token".into(),
                metadata: json!({"sourcePath": "~/.codex/auth.json"}),
            })
            .expect("upsert credential");
        assert_eq!(summary.id, "cred-1");
        assert!(summary.has_secret);

        let summaries = registry.list_summaries().expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].metadata["sourcePath"], "~/.codex/auth.json");

        let revealed = registry
            .reveal("cred-1")
            .expect("reveal")
            .expect("credential exists");
        assert_eq!(revealed.secret, "secret-token");

        assert!(registry.delete("cred-1").expect("delete"));
        assert!(registry.list_summaries().expect("empty").is_empty());
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

    fn inventory_source(
        snapshot: &CallAnalyticsInventorySnapshot,
        source: CallAnalyticsSource,
    ) -> &CallAnalyticsInventorySourceStatus {
        snapshot
            .sources
            .iter()
            .find(|row| row.source == source)
            .expect("inventory source")
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
