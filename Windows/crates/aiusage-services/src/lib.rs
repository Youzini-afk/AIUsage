use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsString,
    fs,
    io::{self, BufRead},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use aiusage_core::{
    claude_has_managed_entries, inject_claude_managed_settings, inject_codex_managed_config,
    inject_opencode_managed_config_with_base, opencode_has_managed_entries, parse_json_or_jsonc,
    strip_claude_managed_settings, strip_codex_managed_blocks, strip_opencode_managed_entries,
    ClaudeManagedSettings, CodexManagedConfig, CredentialKind, OpenCodeManagedNode,
};
use aiusage_platform::{
    AppPaths, AutostartManager, CertificateTrustStore, CredentialVault, FilePermissionGuard,
    PlatformError, PlatformResult,
};
use aiusage_proxy::ProxyUsage;
use chrono::{DateTime, Local, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
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
    #[error("certificate generation failed: {0}")]
    Certificate(String),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

pub const PROXY_USAGE_ARCHIVE_VERSION: u32 = 1;
pub const APP_SETTINGS_VERSION: u32 = 1;
pub const DIAGNOSTICS_EXPORT_VERSION: u32 = 1;
pub const LOCAL_CERTIFICATE_AUTHORITY_VERSION: u32 = 1;
const DIAGNOSTICS_FILE_SCAN_LIMIT: usize = 5_000;
const DIAGNOSTICS_RECENT_FILE_LIMIT: usize = 25;

#[derive(Clone, Debug, Default)]
pub struct NoopFilePermissionGuard;

impl FilePermissionGuard for NoopFilePermissionGuard {
    fn restrict_current_user(&self, _path: &Path) -> PlatformResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoopAutostartManager;

impl AutostartManager for NoopAutostartManager {
    fn is_enabled(&self) -> PlatformResult<bool> {
        Ok(false)
    }

    fn set_enabled(&self, _enabled: bool) -> PlatformResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoopCertificateTrustStore;

impl CertificateTrustStore for NoopCertificateTrustStore {
    fn is_certificate_trusted(&self, _sha256_thumbprint: &str) -> PlatformResult<bool> {
        Ok(false)
    }

    fn trust_certificate_der(&self, _certificate_der: &[u8]) -> PlatformResult<()> {
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
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppLanguage {
    En,
    Zh,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsDocument {
    pub version: u32,
    pub theme_mode: ThemeMode,
    pub language: AppLanguage,
    pub auto_refresh_interval_secs: u32,
    pub proxy_auto_restore_on_launch: bool,
    pub minimize_to_tray_on_close: bool,
    pub keep_running_in_background: bool,
    pub launch_at_login: bool,
}

impl Default for AppSettingsDocument {
    fn default() -> Self {
        Self {
            version: APP_SETTINGS_VERSION,
            theme_mode: ThemeMode::System,
            language: AppLanguage::En,
            auto_refresh_interval_secs: 300,
            proxy_auto_restore_on_launch: false,
            minimize_to_tray_on_close: true,
            keep_running_in_background: true,
            launch_at_login: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsSnapshot {
    pub settings: AppSettingsDocument,
    pub settings_path: String,
    pub autostart_enabled: bool,
    pub autostart_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPathSummary {
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsFileSummary {
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub modified_at_epoch_ms: Option<u128>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExportSnapshot {
    pub version: u32,
    pub generated_at_epoch_ms: u128,
    pub export_path: String,
    pub paths: Vec<DiagnosticsPathSummary>,
    pub recent_files: Vec<DiagnosticsFileSummary>,
    pub warning_messages: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCertificateAuthoritySnapshot {
    pub version: u32,
    pub generated_at_epoch_ms: u128,
    pub certificate_dir: String,
    pub certificate_der_path: String,
    pub certificate_pem_path: String,
    pub private_key_path: String,
    pub certificate_exists: bool,
    pub private_key_exists: bool,
    pub sha256_thumbprint: Option<String>,
    pub trusted_current_user_root: bool,
    pub warning_messages: Vec<String>,
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

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CallAnalyticsKind {
    Mcp,
    Skill,
    Builtin,
    WebSearch,
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsEntry {
    pub source: CallAnalyticsSource,
    pub kind: CallAnalyticsKind,
    pub name: String,
    pub server: Option<String>,
    pub agent: Option<String>,
    pub day_key: String,
    pub count: u64,
    pub outcome_known_count: u64,
    pub success_count: u64,
    pub duration_sample_count: u64,
    pub duration_ms_total: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsInstalledItem {
    pub source: CallAnalyticsSource,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsAgentInvocation {
    pub source: CallAnalyticsSource,
    pub agent: String,
    pub day_key: String,
    pub count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsSourceScanStatus {
    pub source: CallAnalyticsSource,
    pub available: bool,
    pub event_count: u64,
    pub files_scanned: usize,
    pub error_code: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallAnalyticsSnapshot {
    pub generated_at_epoch_ms: u128,
    pub range_key: String,
    pub entries: Vec<CallAnalyticsEntry>,
    pub installed_skills: Vec<CallAnalyticsInstalledItem>,
    pub installed_mcp_servers: Vec<CallAnalyticsInstalledItem>,
    pub agent_invocations: Vec<CallAnalyticsAgentInvocation>,
    pub sources: Vec<CallAnalyticsSourceScanStatus>,
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

#[derive(Clone, Debug)]
pub struct AppSettingsService<P, A = NoopAutostartManager> {
    paths: P,
    autostart: A,
}

impl<P> AppSettingsService<P, NoopAutostartManager>
where
    P: AppPaths,
{
    pub fn new(paths: P) -> Self {
        Self {
            paths,
            autostart: NoopAutostartManager,
        }
    }
}

impl<P, A> AppSettingsService<P, A>
where
    P: AppPaths,
    A: AutostartManager,
{
    pub fn with_autostart(paths: P, autostart: A) -> Self {
        Self { paths, autostart }
    }

    pub fn snapshot(&self) -> ServiceResult<AppSettingsSnapshot> {
        self.snapshot_with_settings(self.load_settings()?)
    }

    pub fn save(&self, mut settings: AppSettingsDocument) -> ServiceResult<AppSettingsSnapshot> {
        settings.version = APP_SETTINGS_VERSION;
        validate_app_settings(&settings)?;
        self.autostart.set_enabled(settings.launch_at_login)?;
        let path = self.settings_path()?;
        write_json_atomically(&path, &serde_json::to_value(&settings)?)?;
        self.snapshot_with_settings(settings)
    }

    fn snapshot_with_settings(
        &self,
        mut settings: AppSettingsDocument,
    ) -> ServiceResult<AppSettingsSnapshot> {
        let (autostart_enabled, autostart_error) = match self.autostart.is_enabled() {
            Ok(enabled) => {
                settings.launch_at_login = enabled;
                (enabled, None)
            }
            Err(error) => (settings.launch_at_login, Some(error.to_string())),
        };
        Ok(AppSettingsSnapshot {
            settings,
            settings_path: display_path(&self.settings_path()?)?,
            autostart_enabled,
            autostart_error,
        })
    }

    fn load_settings(&self) -> ServiceResult<AppSettingsDocument> {
        let path = self.settings_path()?;
        match read_text_if_exists(&path)? {
            Some(text) => {
                let mut settings: AppSettingsDocument = serde_json::from_str(&text)?;
                settings.version = APP_SETTINGS_VERSION;
                validate_app_settings(&settings)?;
                Ok(settings)
            }
            None => Ok(AppSettingsDocument::default()),
        }
    }

    fn settings_path(&self) -> ServiceResult<PathBuf> {
        Ok(self.paths.app_config_dir()?.join("settings.json"))
    }
}

#[derive(Clone, Debug)]
pub struct DiagnosticsExportService<P, G = NoopFilePermissionGuard> {
    paths: P,
    permissions: G,
}

impl<P> DiagnosticsExportService<P, NoopFilePermissionGuard>
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

impl<P, G> DiagnosticsExportService<P, G>
where
    P: AppPaths,
    G: FilePermissionGuard,
{
    pub fn with_permissions(paths: P, permissions: G) -> Self {
        Self { paths, permissions }
    }

    pub fn export(&self) -> ServiceResult<DiagnosticsExportSnapshot> {
        let generated_at_epoch_ms = epoch_ms();
        let app_config_dir = self.paths.app_config_dir()?;
        let app_data_dir = self.paths.app_data_dir()?;
        let app_cache_dir = self.paths.app_cache_dir()?;
        let logs_dir = app_data_dir.join("logs");
        let proxy_logs_dir = app_data_dir.join("proxy-logs");
        let usage_archive_dir = app_config_dir.join("usage-archive");
        let diagnostics_dir = app_data_dir.join("diagnostics");
        fs::create_dir_all(&diagnostics_dir)?;

        let export_path =
            diagnostics_dir.join(format!("aiusage-diagnostics-{generated_at_epoch_ms}.json"));
        let mut warning_messages = Vec::new();

        let paths = [
            ("App config", app_config_dir.clone()),
            ("App data", app_data_dir.clone()),
            ("Cache", app_cache_dir.clone()),
            ("Logs", logs_dir.clone()),
            ("Proxy logs", proxy_logs_dir.clone()),
            ("Usage archive", usage_archive_dir.clone()),
            ("Diagnostics", diagnostics_dir.clone()),
        ]
        .into_iter()
        .map(|(label, path)| diagnostics_path_summary(label, &path, &mut warning_messages))
        .collect::<ServiceResult<Vec<_>>>()?;

        let mut recent_files = Vec::new();
        collect_diagnostics_files(
            "Settings",
            &app_config_dir.join("settings.json"),
            &mut recent_files,
            &mut warning_messages,
        )?;
        collect_diagnostics_files(
            "Usage archive",
            &usage_archive_dir,
            &mut recent_files,
            &mut warning_messages,
        )?;
        collect_diagnostics_files("Logs", &logs_dir, &mut recent_files, &mut warning_messages)?;
        collect_diagnostics_files(
            "Proxy logs",
            &proxy_logs_dir,
            &mut recent_files,
            &mut warning_messages,
        )?;
        recent_files.sort_by(|left, right| {
            right
                .modified_at_epoch_ms
                .unwrap_or_default()
                .cmp(&left.modified_at_epoch_ms.unwrap_or_default())
                .then_with(|| left.path.cmp(&right.path))
        });
        recent_files.truncate(DIAGNOSTICS_RECENT_FILE_LIMIT);

        let snapshot = DiagnosticsExportSnapshot {
            version: DIAGNOSTICS_EXPORT_VERSION,
            generated_at_epoch_ms,
            export_path: display_path(&export_path)?,
            paths,
            recent_files,
            warning_messages,
        };
        write_json_atomically(&export_path, &serde_json::to_value(&snapshot)?)?;
        self.permissions
            .restrict_current_user(&export_path)
            .map_err(ServiceError::Platform)?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug)]
pub struct LocalCertificateAuthorityService<
    P,
    G = NoopFilePermissionGuard,
    T = NoopCertificateTrustStore,
> {
    paths: P,
    permissions: G,
    trust_store: T,
}

impl<P> LocalCertificateAuthorityService<P, NoopFilePermissionGuard, NoopCertificateTrustStore>
where
    P: AppPaths,
{
    pub fn new(paths: P) -> Self {
        Self {
            paths,
            permissions: NoopFilePermissionGuard,
            trust_store: NoopCertificateTrustStore,
        }
    }
}

impl<P, G, T> LocalCertificateAuthorityService<P, G, T>
where
    P: AppPaths,
    G: FilePermissionGuard,
    T: CertificateTrustStore,
{
    pub fn with_platform(paths: P, permissions: G, trust_store: T) -> Self {
        Self {
            paths,
            permissions,
            trust_store,
        }
    }

    pub fn snapshot(&self) -> ServiceResult<LocalCertificateAuthoritySnapshot> {
        self.snapshot_for_paths(&self.certificate_paths()?)
    }

    pub fn ensure(&self) -> ServiceResult<LocalCertificateAuthoritySnapshot> {
        let paths = self.certificate_paths()?;
        if !paths.certificate_der_path.exists() || !paths.private_key_path.exists() {
            self.generate_local_ca(&paths)?;
        }
        self.snapshot_for_paths(&paths)
    }

    pub fn trust_current_user_root(&self) -> ServiceResult<LocalCertificateAuthoritySnapshot> {
        let paths = self.certificate_paths()?;
        if !paths.certificate_der_path.exists() || !paths.private_key_path.exists() {
            self.generate_local_ca(&paths)?;
        }
        let certificate_der = fs::read(&paths.certificate_der_path)?;
        self.trust_store.trust_certificate_der(&certificate_der)?;
        self.snapshot_for_paths(&paths)
    }

    fn certificate_paths(&self) -> ServiceResult<LocalCertificateAuthorityPaths> {
        let certificate_dir = self.paths.app_config_dir()?.join("certificates");
        Ok(LocalCertificateAuthorityPaths {
            certificate_der_path: certificate_dir.join("aiusage-local-root-ca.der"),
            certificate_pem_path: certificate_dir.join("aiusage-local-root-ca.pem"),
            private_key_path: certificate_dir.join("aiusage-local-root-ca-key.pem"),
            certificate_dir,
        })
    }

    fn generate_local_ca(&self, paths: &LocalCertificateAuthorityPaths) -> ServiceResult<()> {
        fs::create_dir_all(&paths.certificate_dir)?;

        let key_pair =
            KeyPair::generate().map_err(|error| ServiceError::Certificate(error.to_string()))?;
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, "AIUsage Local Proxy Root CA");

        let mut params = CertificateParams::new(vec!["AIUsage Local Proxy Root CA".to_string()])
            .map_err(|error| ServiceError::Certificate(error.to_string()))?;
        params.distinguished_name = distinguished_name;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let certificate = params
            .self_signed(&key_pair)
            .map_err(|error| ServiceError::Certificate(error.to_string()))?;
        write_bytes_atomically(&paths.certificate_der_path, certificate.der().as_ref())?;
        write_text_atomically(&paths.certificate_pem_path, &certificate.pem())?;
        write_text_atomically(&paths.private_key_path, &key_pair.serialize_pem())?;

        self.permissions
            .restrict_current_user(&paths.certificate_der_path)?;
        self.permissions
            .restrict_current_user(&paths.certificate_pem_path)?;
        self.permissions
            .restrict_current_user(&paths.private_key_path)?;
        Ok(())
    }

    fn snapshot_for_paths(
        &self,
        paths: &LocalCertificateAuthorityPaths,
    ) -> ServiceResult<LocalCertificateAuthoritySnapshot> {
        let certificate_exists = paths.certificate_der_path.exists();
        let private_key_exists = paths.private_key_path.exists();
        let mut warning_messages = Vec::new();

        let sha256_thumbprint = if certificate_exists {
            let certificate_der = fs::read(&paths.certificate_der_path)?;
            Some(sha256_thumbprint(&certificate_der))
        } else {
            None
        };

        if certificate_exists && !private_key_exists {
            warning_messages.push("Certificate exists but private key is missing".to_string());
        }
        if private_key_exists && !certificate_exists {
            warning_messages.push("Private key exists but certificate is missing".to_string());
        }

        let trusted_current_user_root = match sha256_thumbprint.as_deref() {
            Some(thumbprint) => match self.trust_store.is_certificate_trusted(thumbprint) {
                Ok(trusted) => trusted,
                Err(error) => {
                    warning_messages.push(format!("Could not inspect CurrentUser Root: {error}"));
                    false
                }
            },
            None => false,
        };

        Ok(LocalCertificateAuthoritySnapshot {
            version: LOCAL_CERTIFICATE_AUTHORITY_VERSION,
            generated_at_epoch_ms: epoch_ms(),
            certificate_dir: display_path(&paths.certificate_dir)?,
            certificate_der_path: display_path(&paths.certificate_der_path)?,
            certificate_pem_path: display_path(&paths.certificate_pem_path)?,
            private_key_path: display_path(&paths.private_key_path)?,
            certificate_exists,
            private_key_exists,
            sha256_thumbprint,
            trusted_current_user_root,
            warning_messages,
        })
    }
}

#[derive(Clone, Debug)]
struct LocalCertificateAuthorityPaths {
    certificate_dir: PathBuf,
    certificate_der_path: PathBuf,
    certificate_pem_path: PathBuf,
    private_key_path: PathBuf,
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
pub struct CallAnalyticsService<P> {
    paths: P,
}

impl<P> CallAnalyticsService<P>
where
    P: AppPaths + Clone,
{
    pub fn new(paths: P) -> Self {
        Self { paths }
    }

    pub fn snapshot(&self) -> ServiceResult<CallAnalyticsSnapshot> {
        let inventory = CallAnalyticsInventoryService::new(self.paths.clone()).snapshot()?;
        let mut entries = Vec::new();
        let mut sources = Vec::new();
        let mut agent_invocations = Vec::new();

        let (claude_entries, claude_status, claude_agents) = self.collect_claude_calls()?;
        entries.extend(claude_entries);
        sources.push(claude_status);
        agent_invocations.extend(claude_agents);

        let (codex_entries, codex_status) = self.collect_codex_calls()?;
        entries.extend(codex_entries);
        sources.push(codex_status);

        let opencode_servers = inventory
            .sources
            .iter()
            .find(|row| row.source == CallAnalyticsSource::OpenCode)
            .map(|row| {
                row.mcp_server_names
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let (opencode_entries, opencode_status) = self.collect_opencode_calls(&opencode_servers)?;
        entries.extend(opencode_entries);
        sources.push(opencode_status);

        entries.sort_by(|left, right| {
            (
                &left.day_key,
                source_sort_key(&left.source),
                kind_sort_key(&left.kind),
                &left.name,
                &left.agent,
            )
                .cmp(&(
                    &right.day_key,
                    source_sort_key(&right.source),
                    kind_sort_key(&right.kind),
                    &right.name,
                    &right.agent,
                ))
        });
        agent_invocations.sort_by(|left, right| {
            (&left.day_key, source_sort_key(&left.source), &left.agent).cmp(&(
                &right.day_key,
                source_sort_key(&right.source),
                &right.agent,
            ))
        });

        Ok(CallAnalyticsSnapshot {
            generated_at_epoch_ms: epoch_ms(),
            range_key: "all".into(),
            entries,
            installed_skills: inventory
                .sources
                .iter()
                .flat_map(|row| {
                    row.skill_names
                        .iter()
                        .cloned()
                        .map(|name| CallAnalyticsInstalledItem {
                            source: row.source.clone(),
                            name,
                        })
                })
                .collect(),
            installed_mcp_servers: inventory
                .sources
                .iter()
                .flat_map(|row| {
                    row.mcp_server_names
                        .iter()
                        .cloned()
                        .map(|name| CallAnalyticsInstalledItem {
                            source: row.source.clone(),
                            name,
                        })
                })
                .collect(),
            agent_invocations,
            sources,
        })
    }

    fn collect_claude_calls(
        &self,
    ) -> ServiceResult<(
        Vec<CallAnalyticsEntry>,
        CallAnalyticsSourceScanStatus,
        Vec<CallAnalyticsAgentInvocation>,
    )> {
        let roots = vec![
            self.paths
                .user_home()?
                .join(".config")
                .join("claude")
                .join("projects"),
            self.paths.claude_home()?.join("projects"),
        ];
        let existing_roots = roots
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        if existing_roots.is_empty() {
            return Ok((
                Vec::new(),
                CallAnalyticsSourceScanStatus {
                    source: CallAnalyticsSource::Claude,
                    available: false,
                    event_count: 0,
                    files_scanned: 0,
                    error_code: None,
                    warnings: Vec::new(),
                },
                Vec::new(),
            ));
        }

        let mut warnings = Vec::new();
        let files = collect_jsonl_files(&existing_roots, &mut warnings);
        let mut accumulator = CallEventAccumulator::default();
        let mut invocations_by_day = BTreeMap::<(String, String), u64>::new();

        for file in &files {
            let fallback_day_key = file_day_key(file);
            let is_subagent_file = has_path_component(file, "subagents");
            let subagent_type = is_subagent_file
                .then(|| read_claude_subagent_type(file))
                .flatten();
            let agent_name = if is_subagent_file {
                subagent_type.clone().unwrap_or_else(|| "subagent".into())
            } else {
                "main".into()
            };
            let mut pending = HashMap::<String, PendingCall>::new();
            let mut earliest_day_key: Option<String> = None;
            if let Err(error) = for_each_matching_line(
                file,
                &["\"tool_use\"", "\"tool_result\""],
                4 * 1024 * 1024,
                |line| {
                    parse_claude_line(
                        line,
                        &fallback_day_key,
                        is_subagent_file,
                        subagent_type.as_deref(),
                        &mut pending,
                        &mut earliest_day_key,
                        &mut accumulator,
                    );
                },
            ) {
                warnings.push(format!("{}: {}", file.display(), error));
            }

            for call in pending.into_values() {
                accumulator.add(call);
            }
            let invocation_day = earliest_day_key.unwrap_or(fallback_day_key);
            *invocations_by_day
                .entry((invocation_day, agent_name))
                .or_default() += 1;
        }

        let event_count = accumulator.event_count;
        Ok((
            accumulator.entries(),
            CallAnalyticsSourceScanStatus {
                source: CallAnalyticsSource::Claude,
                available: true,
                event_count,
                files_scanned: files.len(),
                error_code: None,
                warnings,
            },
            invocations_by_day
                .into_iter()
                .map(|((day_key, agent), count)| CallAnalyticsAgentInvocation {
                    source: CallAnalyticsSource::Claude,
                    agent,
                    day_key,
                    count,
                })
                .collect(),
        ))
    }

    fn collect_codex_calls(
        &self,
    ) -> ServiceResult<(Vec<CallAnalyticsEntry>, CallAnalyticsSourceScanStatus)> {
        let codex_home = self.paths.codex_home()?;
        let roots = vec![
            codex_home.join("sessions"),
            codex_home.join("archived_sessions"),
        ];
        let existing_roots = roots
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        if existing_roots.is_empty() {
            return Ok((
                Vec::new(),
                CallAnalyticsSourceScanStatus {
                    source: CallAnalyticsSource::Codex,
                    available: false,
                    event_count: 0,
                    files_scanned: 0,
                    error_code: None,
                    warnings: Vec::new(),
                },
            ));
        }

        let mut warnings = Vec::new();
        let files = collect_jsonl_files(&existing_roots, &mut warnings);
        let mut accumulator = CallEventAccumulator::default();

        for file in &files {
            let fallback_day_key = file_day_key(file);
            if let Err(error) = for_each_matching_line(
                file,
                &["\"function_call\"", "\"mcp_tool_call_end\""],
                256 * 1024,
                |line| parse_codex_line(line, &fallback_day_key, &mut accumulator),
            ) {
                warnings.push(format!("{}: {}", file.display(), error));
            }
        }

        let event_count = accumulator.event_count;
        Ok((
            accumulator.entries(),
            CallAnalyticsSourceScanStatus {
                source: CallAnalyticsSource::Codex,
                available: true,
                event_count,
                files_scanned: files.len(),
                error_code: None,
                warnings,
            },
        ))
    }

    fn collect_opencode_calls(
        &self,
        known_mcp_servers: &BTreeSet<String>,
    ) -> ServiceResult<(Vec<CallAnalyticsEntry>, CallAnalyticsSourceScanStatus)> {
        let data_dirs =
            CallAnalyticsInventoryService::new(self.paths.clone()).opencode_data_dirs()?;
        let Some(database_path) = data_dirs
            .into_iter()
            .map(|dir| dir.join("opencode.db"))
            .find(|path| path.is_file())
        else {
            return Ok((
                Vec::new(),
                CallAnalyticsSourceScanStatus {
                    source: CallAnalyticsSource::OpenCode,
                    available: false,
                    event_count: 0,
                    files_scanned: 0,
                    error_code: None,
                    warnings: Vec::new(),
                },
            ));
        };

        let snapshot_path = match copy_sqlite_snapshot(&database_path) {
            Ok(path) => path,
            Err(error) => {
                return Ok((
                    Vec::new(),
                    CallAnalyticsSourceScanStatus {
                        source: CallAnalyticsSource::OpenCode,
                        available: true,
                        event_count: 0,
                        files_scanned: 0,
                        error_code: Some("db_snapshot_failed".into()),
                        warnings: vec![format!("{}: {}", database_path.display(), error)],
                    },
                ));
            }
        };

        let mut accumulator = CallEventAccumulator::default();
        let mut warnings = Vec::new();
        let result =
            collect_opencode_tool_parts(&snapshot_path, known_mcp_servers, &mut accumulator);
        cleanup_sqlite_snapshot(&snapshot_path);

        if let Err(error) = result {
            warnings.push(format!("{}: {}", database_path.display(), error));
            return Ok((
                Vec::new(),
                CallAnalyticsSourceScanStatus {
                    source: CallAnalyticsSource::OpenCode,
                    available: true,
                    event_count: 0,
                    files_scanned: 1,
                    error_code: Some("db_query_failed".into()),
                    warnings,
                },
            ));
        }

        let event_count = accumulator.event_count;
        Ok((
            accumulator.entries(),
            CallAnalyticsSourceScanStatus {
                source: CallAnalyticsSource::OpenCode,
                available: true,
                event_count,
                files_scanned: 1,
                error_code: None,
                warnings,
            },
        ))
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

#[derive(Clone, Debug)]
struct PendingCall {
    source: CallAnalyticsSource,
    kind: CallAnalyticsKind,
    name: String,
    server: Option<String>,
    agent: Option<String>,
    day_key: String,
    success: Option<bool>,
    duration_ms: Option<f64>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CallEventKey {
    source: CallAnalyticsSource,
    kind: CallAnalyticsKind,
    name: String,
    server: Option<String>,
    agent: Option<String>,
    day_key: String,
}

#[derive(Clone, Debug, Default)]
struct CallEventAggregate {
    count: u64,
    outcome_known_count: u64,
    success_count: u64,
    duration_sample_count: u64,
    duration_ms_total: f64,
}

#[derive(Clone, Debug, Default)]
struct CallEventAccumulator {
    counts: HashMap<CallEventKey, CallEventAggregate>,
    event_count: u64,
}

impl CallEventAccumulator {
    fn add(&mut self, call: PendingCall) {
        let key = CallEventKey {
            source: call.source,
            kind: call.kind,
            name: call.name,
            server: call.server,
            agent: call.agent,
            day_key: call.day_key,
        };
        let aggregate = self.counts.entry(key).or_default();
        aggregate.count = aggregate.count.saturating_add(1);
        if let Some(success) = call.success {
            aggregate.outcome_known_count = aggregate.outcome_known_count.saturating_add(1);
            if success {
                aggregate.success_count = aggregate.success_count.saturating_add(1);
            }
        }
        if let Some(duration_ms) = call.duration_ms.filter(|duration| *duration >= 0.0) {
            aggregate.duration_sample_count = aggregate.duration_sample_count.saturating_add(1);
            aggregate.duration_ms_total += duration_ms;
        }
        self.event_count = self.event_count.saturating_add(1);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_parts(
        &mut self,
        source: CallAnalyticsSource,
        kind: CallAnalyticsKind,
        name: impl Into<String>,
        server: Option<String>,
        agent: Option<String>,
        day_key: impl Into<String>,
        success: Option<bool>,
        duration_ms: Option<f64>,
    ) {
        self.add(PendingCall {
            source,
            kind,
            name: name.into(),
            server,
            agent,
            day_key: day_key.into(),
            success,
            duration_ms,
        });
    }

    fn entries(self) -> Vec<CallAnalyticsEntry> {
        self.counts
            .into_iter()
            .map(|(key, aggregate)| CallAnalyticsEntry {
                source: key.source,
                kind: key.kind,
                name: key.name,
                server: key.server,
                agent: key.agent,
                day_key: key.day_key,
                count: aggregate.count,
                outcome_known_count: aggregate.outcome_known_count,
                success_count: aggregate.success_count,
                duration_sample_count: aggregate.duration_sample_count,
                duration_ms_total: aggregate.duration_ms_total,
            })
            .collect()
    }
}

fn collect_jsonl_files(roots: &[PathBuf], warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::<PathBuf>::new();
    for root in roots {
        collect_jsonl_files_in_dir(root, &mut seen, &mut files, warnings);
    }
    files
}

fn collect_jsonl_files_in_dir(
    directory: &Path,
    seen: &mut BTreeSet<PathBuf>,
    files: &mut Vec<PathBuf>,
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
            collect_jsonl_files_in_dir(&path, seen, files, warnings);
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
            && seen.insert(path.clone())
        {
            files.push(path);
        }
    }
}

fn for_each_matching_line(
    path: &Path,
    needles: &[&str],
    max_line_bytes: usize,
    mut on_line: impl FnMut(&[u8]),
) -> io::Result<()> {
    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let needle_bytes = needles
        .iter()
        .map(|needle| needle.as_bytes())
        .collect::<Vec<_>>();
    for line in reader.split(b'\n') {
        let mut line = line?;
        if line.ends_with(b"\r") {
            line.pop();
        }
        if line.len() > max_line_bytes {
            line.truncate(max_line_bytes);
        }
        if needle_bytes.is_empty()
            || needle_bytes
                .iter()
                .any(|needle| contains_bytes(&line, needle))
        {
            on_line(&line);
        }
    }
    Ok(())
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn parse_claude_line(
    line: &[u8],
    fallback_day_key: &str,
    is_subagent_file: bool,
    subagent_type: Option<&str>,
    pending: &mut HashMap<String, PendingCall>,
    earliest_day_key: &mut Option<String>,
    accumulator: &mut CallEventAccumulator,
) {
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        return;
    };
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return;
    };
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };

    match kind {
        "assistant" => {
            let day_key = value
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(day_key_from_iso)
                .unwrap_or_else(|| fallback_day_key.to_string());
            if earliest_day_key
                .as_ref()
                .map(|existing| &day_key < existing)
                .unwrap_or(true)
            {
                *earliest_day_key = Some(day_key.clone());
            }
            let agent = if is_subagent_file {
                subagent_type.unwrap_or("subagent").to_string()
            } else if value
                .get("isSidechain")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "subagent".into()
            } else {
                "main".into()
            };

            for item in content {
                if item.get("type").and_then(Value::as_str) != Some("tool_use") {
                    continue;
                }
                let Some(raw_name) = item.get("name").and_then(Value::as_str) else {
                    continue;
                };
                if raw_name.trim().is_empty() {
                    continue;
                }
                let call = make_claude_call(raw_name, item.get("input"), &agent, &day_key);
                if let Some(id) = item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                {
                    pending.insert(id.to_string(), call);
                } else {
                    accumulator.add(call);
                }
            }
        }
        "user" => {
            for item in content {
                if item.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                let Some(id) = item.get("tool_use_id").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(mut call) = pending.remove(id) {
                    call.success = Some(
                        !item
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    );
                    accumulator.add(call);
                }
            }
        }
        _ => {}
    }
}

fn make_claude_call(
    raw_name: &str,
    input: Option<&Value>,
    agent: &str,
    day_key: &str,
) -> PendingCall {
    if let Some((server, tool)) = parse_claude_mcp(raw_name) {
        return PendingCall {
            source: CallAnalyticsSource::Claude,
            kind: CallAnalyticsKind::Mcp,
            name: mcp_display_name(&server, &tool),
            server: Some(server),
            agent: Some(agent.to_string()),
            day_key: day_key.to_string(),
            success: None,
            duration_ms: None,
        };
    }

    if raw_name == "Skill" {
        let skill_name = input
            .and_then(|input| input.get("skill"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("(unknown)");
        return PendingCall {
            source: CallAnalyticsSource::Claude,
            kind: CallAnalyticsKind::Skill,
            name: skill_name.to_string(),
            server: None,
            agent: Some(agent.to_string()),
            day_key: day_key.to_string(),
            success: None,
            duration_ms: None,
        };
    }

    PendingCall {
        source: CallAnalyticsSource::Claude,
        kind: if matches!(raw_name, "WebSearch" | "WebFetch") {
            CallAnalyticsKind::WebSearch
        } else {
            CallAnalyticsKind::Builtin
        },
        name: raw_name.to_string(),
        server: None,
        agent: Some(agent.to_string()),
        day_key: day_key.to_string(),
        success: None,
        duration_ms: None,
    }
}

fn parse_codex_line(line: &[u8], fallback_day_key: &str, accumulator: &mut CallEventAccumulator) {
    let text = String::from_utf8_lossy(line);
    let day_key = extract_json_string(&text, "timestamp")
        .and_then(|timestamp| day_key_from_iso(&timestamp))
        .unwrap_or_else(|| fallback_day_key.to_string());

    if text.contains("\"mcp_tool_call_end\"") {
        let Some(invocation) = extract_json_object_text(&text, "invocation") else {
            return;
        };
        let Some(server) = extract_json_string(&invocation, "server")
            .map(|server| server.trim().to_string())
            .filter(|server| !server.is_empty())
        else {
            return;
        };
        let Some(tool) = extract_json_string(&invocation, "tool")
            .map(|tool| tool.trim().to_string())
            .filter(|tool| !tool.is_empty())
        else {
            return;
        };
        let success = extract_json_object_text(&text, "result")
            .and_then(|result| first_json_object_key(&result))
            .and_then(|key| match key.as_str() {
                "Ok" => Some(true),
                "Err" => Some(false),
                _ => None,
            });
        let duration_ms = extract_json_object_text(&text, "duration").and_then(|duration| {
            let secs = extract_json_i64(&duration, "secs").unwrap_or(0);
            let nanos = extract_json_i64(&duration, "nanos").unwrap_or(0);
            (secs != 0 || nanos != 0).then_some(secs as f64 * 1000.0 + nanos as f64 / 1_000_000.0)
        });
        accumulator.add_parts(
            CallAnalyticsSource::Codex,
            CallAnalyticsKind::Mcp,
            mcp_display_name(&server, &tool),
            Some(server),
            None,
            day_key,
            success,
            duration_ms,
        );
        return;
    }

    if text.contains("\"function_call\"") {
        if let Some(name) = extract_json_string(&text, "name")
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
        {
            accumulator.add_parts(
                CallAnalyticsSource::Codex,
                CallAnalyticsKind::Builtin,
                name,
                None,
                None,
                day_key.clone(),
                None,
                None,
            );
        }
        for skill in codex_skill_reads(&text) {
            accumulator.add_parts(
                CallAnalyticsSource::Codex,
                CallAnalyticsKind::Skill,
                skill,
                None,
                None,
                day_key.clone(),
                None,
                None,
            );
        }
    }
}

fn collect_opencode_tool_parts(
    database_path: &Path,
    known_mcp_servers: &BTreeSet<String>,
    accumulator: &mut CallEventAccumulator,
) -> rusqlite::Result<()> {
    let connection = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = connection
        .prepare("SELECT time_created, data FROM part WHERE data LIKE '%\"type\":\"tool\"%'")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let millis: i64 = row.get(0)?;
        let data: String = row.get(1)?;
        parse_opencode_part(&data, millis, known_mcp_servers, accumulator);
    }
    Ok(())
}

fn parse_opencode_part(
    data: &str,
    millis: i64,
    known_mcp_servers: &BTreeSet<String>,
    accumulator: &mut CallEventAccumulator,
) {
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return;
    };
    if value.get("type").and_then(Value::as_str) != Some("tool") {
        return;
    }
    let Some(tool) = value
        .get("tool")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tool| !tool.is_empty())
    else {
        return;
    };
    let day_key = day_key_from_epoch_millis(millis);
    let state = value.get("state");
    let success = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        .and_then(|status| match status.to_ascii_lowercase().as_str() {
            "completed" => Some(true),
            "error" => Some(false),
            _ => None,
        });
    let duration_ms = state.and_then(|state| state.get("time")).and_then(|time| {
        let start = time.get("start").and_then(Value::as_f64)?;
        let end = time.get("end").and_then(Value::as_f64)?;
        (end >= start).then_some(end - start)
    });
    classify_opencode_tool(
        tool,
        state,
        &day_key,
        known_mcp_servers,
        success,
        duration_ms,
        accumulator,
    );
}

fn classify_opencode_tool(
    tool: &str,
    state: Option<&Value>,
    day_key: &str,
    known_mcp_servers: &BTreeSet<String>,
    success: Option<bool>,
    duration_ms: Option<f64>,
    accumulator: &mut CallEventAccumulator,
) {
    const BUILTIN_TOOLS: &[&str] = &[
        "read",
        "write",
        "edit",
        "multiedit",
        "bash",
        "glob",
        "grep",
        "list",
        "webfetch",
        "patch",
        "task",
        "question",
        "todowrite",
        "todoread",
        "invalid",
    ];

    let lower = tool.to_ascii_lowercase();
    if lower == "skill" {
        let skill_name = state
            .and_then(|state| state.get("input"))
            .and_then(|input| input.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("(unknown)");
        accumulator.add_parts(
            CallAnalyticsSource::OpenCode,
            CallAnalyticsKind::Skill,
            skill_name,
            None,
            None,
            day_key,
            success,
            duration_ms,
        );
        return;
    }

    if BUILTIN_TOOLS.contains(&lower.as_str()) {
        accumulator.add_parts(
            CallAnalyticsSource::OpenCode,
            if lower == "webfetch" {
                CallAnalyticsKind::WebSearch
            } else {
                CallAnalyticsKind::Builtin
            },
            tool,
            None,
            None,
            day_key,
            success,
            duration_ms,
        );
        return;
    }

    if let Some((server, tool_name)) = match_known_opencode_server(tool, known_mcp_servers) {
        accumulator.add_parts(
            CallAnalyticsSource::OpenCode,
            CallAnalyticsKind::Mcp,
            mcp_display_name(&server, &tool_name),
            Some(server),
            None,
            day_key,
            success,
            duration_ms,
        );
        return;
    }

    if let Some(separator) = tool.find('_') {
        let server = tool[..separator].trim();
        let tool_name = tool[separator + 1..].trim();
        if !server.is_empty() && !tool_name.is_empty() {
            accumulator.add_parts(
                CallAnalyticsSource::OpenCode,
                CallAnalyticsKind::Mcp,
                mcp_display_name(server, tool_name),
                Some(server.to_string()),
                None,
                day_key,
                success,
                duration_ms,
            );
            return;
        }
    }

    accumulator.add_parts(
        CallAnalyticsSource::OpenCode,
        CallAnalyticsKind::Other,
        tool,
        None,
        None,
        day_key,
        success,
        duration_ms,
    );
}

fn match_known_opencode_server(
    tool: &str,
    known_mcp_servers: &BTreeSet<String>,
) -> Option<(String, String)> {
    let mut servers = known_mcp_servers.iter().collect::<Vec<_>>();
    servers.sort_by_key(|server| std::cmp::Reverse(server.len()));
    for server in servers {
        for candidate in [server.to_string(), server.replace('-', "_")] {
            let prefix = format!("{candidate}_");
            if let Some(tool_name) = tool.strip_prefix(&prefix).filter(|name| !name.is_empty()) {
                return Some((server.clone(), tool_name.to_string()));
            }
        }
    }
    None
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let relative = text[cursor..].find(&needle)?;
        let mut index = cursor + relative + needle.len();
        skip_json_whitespace(bytes, &mut index);
        if bytes.get(index) != Some(&b':') {
            cursor = index;
            continue;
        }
        index += 1;
        skip_json_whitespace(bytes, &mut index);
        if bytes.get(index) != Some(&b'"') {
            cursor = index;
            continue;
        }
        index += 1;
        let mut output = String::new();
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            index += 1;
            if escaped {
                output.push(match byte {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    other => other as char,
                });
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                return Some(output);
            } else {
                output.push(byte as char);
            }
        }
        return None;
    }
    None
}

fn extract_json_i64(text: &str, key: &str) -> Option<i64> {
    let needle = format!("\"{key}\"");
    let bytes = text.as_bytes();
    let relative = text.find(&needle)?;
    let mut index = relative + needle.len();
    skip_json_whitespace(bytes, &mut index);
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    skip_json_whitespace(bytes, &mut index);
    let start = index;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
        index += 1;
    }
    (index > start)
        .then(|| text[start..index].parse().ok())
        .flatten()
}

fn extract_json_object_text(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let bytes = text.as_bytes();
    let relative = text.find(&needle)?;
    let mut index = relative + needle.len();
    skip_json_whitespace(bytes, &mut index);
    if bytes.get(index) != Some(&b':') {
        return None;
    }
    index += 1;
    skip_json_whitespace(bytes, &mut index);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }

    let start = index;
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(text[start..=index].to_string());
            }
        }
        index += 1;
    }
    None
}

fn first_json_object_key(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut index = 0;
    skip_json_whitespace(bytes, &mut index);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    index += 1;
    skip_json_whitespace(bytes, &mut index);
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;
    let start = index;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(text[start..index].to_string());
        }
        index += 1;
    }
    None
}

fn skip_json_whitespace(bytes: &[u8], index: &mut usize) {
    while bytes
        .get(*index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        *index += 1;
    }
}

fn codex_skill_reads(text: &str) -> Vec<String> {
    let text = text.replace("\\/", "/");
    let marker = "/SKILL.md";
    let mut names = BTreeSet::new();
    let mut cursor = 0;
    while let Some(relative) = text[cursor..].find(marker) {
        let marker_start = cursor + relative;
        cursor = marker_start + marker.len();
        let before = &text[..marker_start];
        let Some(name_slash) = before.rfind('/') else {
            continue;
        };
        let name = &before[name_slash + 1..];
        let parent = &before[..name_slash];
        if (parent.ends_with("/skills") || parent == "skills")
            && !name.is_empty()
            && name != ".system"
            && !before.contains("/.system/")
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        {
            names.insert(name.to_string());
        }
    }
    names.into_iter().collect()
}

fn parse_claude_mcp(raw: &str) -> Option<(String, String)> {
    let body = raw.strip_prefix("mcp__")?;
    let Some(separator) = body.find("__") else {
        return Some((body.to_string(), body.to_string()));
    };
    let server = body[..separator].to_string();
    let tool = body[separator + 2..].trim();
    Some((
        server.clone(),
        if tool.is_empty() {
            server
        } else {
            tool.to_string()
        },
    ))
}

fn mcp_display_name(server: &str, tool: &str) -> String {
    format!("{server}/{tool}")
}

fn read_claude_subagent_type(path: &Path) -> Option<String> {
    let meta_path = path.with_extension("meta.json");
    let text = fs::read_to_string(meta_path).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    value
        .get("agentType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
        .map(ToOwned::to_owned)
}

fn has_path_component(path: &Path, needle: &str) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|component| component.eq_ignore_ascii_case(needle))
    })
}

fn copy_sqlite_snapshot(source: &Path) -> io::Result<PathBuf> {
    let snapshot = std::env::temp_dir().join(format!(
        "aiusage-callanalytics-{}-{}.db",
        std::process::id(),
        epoch_ms()
    ));
    fs::copy(source, &snapshot)?;
    for suffix in ["-wal", "-shm"] {
        let source_sidecar = path_with_appended_suffix(source, suffix);
        if source_sidecar.exists() {
            let _ = fs::copy(source_sidecar, path_with_appended_suffix(&snapshot, suffix));
        }
    }
    Ok(snapshot)
}

fn cleanup_sqlite_snapshot(snapshot: &Path) {
    let _ = fs::remove_file(snapshot);
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(path_with_appended_suffix(snapshot, suffix));
    }
}

fn path_with_appended_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = OsString::from(path.as_os_str());
    raw.push(suffix);
    PathBuf::from(raw)
}

fn day_key_from_iso(text: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|date| date.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

fn day_key_from_epoch_millis(millis: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .unwrap_or_else(Utc::now)
        .with_timezone(&Local)
        .format("%Y-%m-%d")
        .to_string()
}

fn file_day_key(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(|modified| {
            DateTime::<Local>::from(modified)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| Local::now().format("%Y-%m-%d").to_string())
}

fn source_sort_key(source: &CallAnalyticsSource) -> u8 {
    match source {
        CallAnalyticsSource::Claude => 0,
        CallAnalyticsSource::Codex => 1,
        CallAnalyticsSource::OpenCode => 2,
    }
}

fn kind_sort_key(kind: &CallAnalyticsKind) -> u8 {
    match kind {
        CallAnalyticsKind::Mcp => 0,
        CallAnalyticsKind::Skill => 1,
        CallAnalyticsKind::Builtin => 2,
        CallAnalyticsKind::WebSearch => 3,
        CallAnalyticsKind::Other => 4,
    }
}

fn validate_app_settings(settings: &AppSettingsDocument) -> ServiceResult<()> {
    const SUPPORTED_REFRESH_INTERVALS: &[u32] = &[30, 60, 180, 300, 600, 900, 1800, 3600, 0];
    if !SUPPORTED_REFRESH_INTERVALS.contains(&settings.auto_refresh_interval_secs) {
        return Err(ServiceError::InvalidRequest(
            "auto_refresh_interval_secs is not supported",
        ));
    }
    Ok(())
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

fn diagnostics_path_summary(
    label: &str,
    path: &Path,
    warning_messages: &mut Vec<String>,
) -> ServiceResult<DiagnosticsPathSummary> {
    let mut summary = DiagnosticsPathSummary {
        label: label.to_string(),
        path: display_path(path)?,
        exists: path.exists(),
        file_count: 0,
        total_bytes: 0,
    };
    if !summary.exists {
        return Ok(summary);
    }

    let mut stack = vec![path.to_path_buf()];
    while let Some(candidate) = stack.pop() {
        let metadata = match fs::metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) => {
                warning_messages.push(format!(
                    "Could not inspect {}: {error}",
                    candidate.display()
                ));
                continue;
            }
        };

        if metadata.is_file() {
            summary.file_count += 1;
            summary.total_bytes = summary.total_bytes.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            let entries = match fs::read_dir(&candidate) {
                Ok(entries) => entries,
                Err(error) => {
                    warning_messages.push(format!(
                        "Could not read directory {}: {error}",
                        candidate.display()
                    ));
                    continue;
                }
            };
            for entry in entries {
                match entry {
                    Ok(entry) => stack.push(entry.path()),
                    Err(error) => {
                        warning_messages.push(format!(
                            "Could not read entry under {}: {error}",
                            candidate.display()
                        ));
                    }
                }
            }
        }

        if summary.file_count >= DIAGNOSTICS_FILE_SCAN_LIMIT {
            warning_messages.push(format!(
                "Stopped scanning {label} after {DIAGNOSTICS_FILE_SCAN_LIMIT} files"
            ));
            break;
        }
    }

    Ok(summary)
}

fn collect_diagnostics_files(
    label: &str,
    path: &Path,
    files: &mut Vec<DiagnosticsFileSummary>,
    warning_messages: &mut Vec<String>,
) -> ServiceResult<()> {
    if !path.exists() {
        return Ok(());
    }

    let mut stack = vec![path.to_path_buf()];
    while let Some(candidate) = stack.pop() {
        let metadata = match fs::metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) => {
                warning_messages.push(format!(
                    "Could not inspect {}: {error}",
                    candidate.display()
                ));
                continue;
            }
        };

        if metadata.is_file() {
            files.push(DiagnosticsFileSummary {
                label: label.to_string(),
                path: display_path(&candidate)?,
                bytes: metadata.len(),
                modified_at_epoch_ms: metadata.modified().ok().and_then(system_time_epoch_ms),
            });
        } else if metadata.is_dir() {
            let entries = match fs::read_dir(&candidate) {
                Ok(entries) => entries,
                Err(error) => {
                    warning_messages.push(format!(
                        "Could not read directory {}: {error}",
                        candidate.display()
                    ));
                    continue;
                }
            };
            for entry in entries {
                match entry {
                    Ok(entry) => stack.push(entry.path()),
                    Err(error) => {
                        warning_messages.push(format!(
                            "Could not read entry under {}: {error}",
                            candidate.display()
                        ));
                    }
                }
            }
        }

        if files.len() >= DIAGNOSTICS_FILE_SCAN_LIMIT {
            warning_messages.push(format!(
                "Stopped collecting diagnostic files after {DIAGNOSTICS_FILE_SCAN_LIMIT} files"
            ));
            break;
        }
    }

    Ok(())
}

fn system_time_epoch_ms(time: std::time::SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
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

fn sha256_thumbprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join("")
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
    use aiusage_platform::{CertificateTrustStore, CredentialVault, PlatformResult};
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

    #[derive(Clone, Debug, Default)]
    struct MemoryAutostart {
        enabled: Arc<Mutex<bool>>,
    }

    impl AutostartManager for MemoryAutostart {
        fn is_enabled(&self) -> PlatformResult<bool> {
            Ok(*self.enabled.lock().expect("autostart lock"))
        }

        fn set_enabled(&self, enabled: bool) -> PlatformResult<()> {
            *self.enabled.lock().expect("autostart lock") = enabled;
            Ok(())
        }
    }

    #[derive(Clone, Debug, Default)]
    struct MemoryCertificateTrustStore {
        trusted: Arc<Mutex<BTreeSet<String>>>,
    }

    impl CertificateTrustStore for MemoryCertificateTrustStore {
        fn is_certificate_trusted(&self, sha256_thumbprint: &str) -> PlatformResult<bool> {
            Ok(self
                .trusted
                .lock()
                .expect("trust lock")
                .contains(sha256_thumbprint))
        }

        fn trust_certificate_der(&self, certificate_der: &[u8]) -> PlatformResult<()> {
            self.trusted
                .lock()
                .expect("trust lock")
                .insert(sha256_thumbprint(certificate_der));
            Ok(())
        }
    }

    #[test]
    fn app_settings_persist_and_sync_autostart() {
        let temp = TempDir::new().expect("tempdir");
        let autostart = MemoryAutostart::default();
        let service = AppSettingsService::with_autostart(
            TestPaths::new(temp.path().to_path_buf()),
            autostart.clone(),
        );

        let initial = service.snapshot().expect("settings snapshot");
        assert_eq!(initial.settings.theme_mode, ThemeMode::System);
        assert!(!initial.settings.launch_at_login);
        assert!(initial.settings_path.ends_with("settings.json"));

        let mut next = initial.settings;
        next.theme_mode = ThemeMode::Dark;
        next.language = AppLanguage::Zh;
        next.proxy_auto_restore_on_launch = true;
        next.launch_at_login = true;
        let saved = service.save(next).expect("save settings");
        assert!(saved.autostart_enabled);
        assert_eq!(saved.settings.theme_mode, ThemeMode::Dark);
        assert!(*autostart.enabled.lock().expect("autostart lock"));

        let stored = fs::read_to_string(
            temp.path()
                .join("appdata")
                .join("AIUsage")
                .join("settings.json"),
        )
        .expect("settings json");
        assert!(stored.contains("\"themeMode\": \"dark\""));
        assert!(stored.contains("\"language\": \"zh\""));
    }

    #[test]
    fn diagnostics_export_writes_secret_free_metadata_report() {
        let temp = TempDir::new().expect("tempdir");
        let paths = TestPaths::new(temp.path().to_path_buf());
        let config_dir = paths.app_config_dir().expect("config dir");
        let local_dir = paths.app_data_dir().expect("local data dir");
        fs::create_dir_all(config_dir.join("usage-archive")).expect("usage archive dir");
        fs::create_dir_all(local_dir.join("logs")).expect("logs dir");
        fs::write(
            config_dir.join("settings.json"),
            r#"{"proxyAutoRestoreOnLaunch":false}"#,
        )
        .expect("settings file");
        fs::write(
            local_dir.join("logs").join("aiusage.log"),
            "token=SHOULD_NOT_APPEAR_IN_DIAGNOSTICS",
        )
        .expect("log file");
        fs::write(
            config_dir
                .join("usage-archive")
                .join("proxy-usage-codex-v1.json"),
            "{}",
        )
        .expect("archive file");

        let service = DiagnosticsExportService::new(paths);
        let snapshot = service.export().expect("diagnostics export");

        assert_eq!(snapshot.version, DIAGNOSTICS_EXPORT_VERSION);
        assert!(Path::new(&snapshot.export_path).exists());
        assert!(snapshot
            .paths
            .iter()
            .any(|path| path.label == "Logs" && path.exists && path.file_count >= 1));
        assert!(snapshot
            .recent_files
            .iter()
            .any(|file| file.path.ends_with("aiusage.log")));

        let report = fs::read_to_string(&snapshot.export_path).expect("diagnostics report");
        assert!(report.contains("aiusage.log"));
        assert!(!report.contains("SHOULD_NOT_APPEAR_IN_DIAGNOSTICS"));
    }

    #[test]
    fn local_certificate_authority_generates_and_tracks_trust() {
        let temp = TempDir::new().expect("tempdir");
        let trust_store = MemoryCertificateTrustStore::default();
        let service = LocalCertificateAuthorityService::with_platform(
            TestPaths::new(temp.path().to_path_buf()),
            NoopFilePermissionGuard,
            trust_store,
        );

        let initial = service.snapshot().expect("initial ca snapshot");
        assert!(!initial.certificate_exists);
        assert!(!initial.private_key_exists);
        assert!(!initial.trusted_current_user_root);

        let prepared = service.ensure().expect("generated local ca");
        assert!(prepared.certificate_exists);
        assert!(prepared.private_key_exists);
        assert!(prepared.sha256_thumbprint.is_some());
        assert!(!prepared.trusted_current_user_root);
        assert!(Path::new(&prepared.certificate_der_path).exists());
        assert!(Path::new(&prepared.certificate_pem_path).exists());
        assert!(Path::new(&prepared.private_key_path).exists());

        let trusted = service.trust_current_user_root().expect("trust local ca");
        assert_eq!(trusted.sha256_thumbprint, prepared.sha256_thumbprint);
        assert!(trusted.trusted_current_user_root);
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
    fn call_analytics_snapshot_aggregates_claude_codex_and_opencode_events() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();

        let claude_project = root.join(".claude").join("projects").join("workspace");
        fs::create_dir_all(&claude_project).expect("claude project dir");
        fs::write(
            claude_project.join("session.jsonl"),
            [
                r#"{"type":"assistant","timestamp":"2026-07-01T10:00:00Z","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"mcp__fs__read","input":{}},{"type":"tool_use","id":"toolu_2","name":"Skill","input":{"skill":"briefing"}},{"type":"tool_use","name":"WebSearch","input":{}}]}}"#,
                r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":false},{"type":"tool_result","tool_use_id":"toolu_2","is_error":true}]}}"#,
            ]
            .join("\n"),
        )
        .expect("claude session");

        let codex_sessions = root.join(".codex").join("sessions");
        fs::create_dir_all(&codex_sessions).expect("codex sessions");
        fs::write(
            codex_sessions.join("session.jsonl"),
            [
                r#"{"timestamp":"2026-07-01T10:01:00Z","event_msg":{"type":"mcp_tool_call_end","invocation":{"server":"docs","tool":"search"},"result":{"Ok":{}},"duration":{"secs":1,"nanos":500000000}}}"#,
                r#"{"timestamp":"2026-07-01T10:02:00Z","response_item":{"payload":{"type":"function_call","name":"exec_command","arguments":"cat C:/Users/me/.codex/skills/review/SKILL.md"}}}"#,
            ]
            .join("\n"),
        )
        .expect("codex session");

        let opencode_config = root.join(".config").join("opencode");
        fs::create_dir_all(&opencode_config).expect("opencode config dir");
        fs::write(
            opencode_config.join("opencode.jsonc"),
            r#"{"mcp":{"context7":{}}}"#,
        )
        .expect("opencode config");
        let opencode_data = root.join("localappdata").join("opencode");
        fs::create_dir_all(&opencode_data).expect("opencode data");
        let db = Connection::open(opencode_data.join("opencode.db")).expect("open db");
        db.execute(
            "CREATE TABLE part (time_created INTEGER NOT NULL, data TEXT NOT NULL)",
            [],
        )
        .expect("create part");
        db.execute(
            "INSERT INTO part (time_created, data) VALUES (?1, ?2)",
            (
                1_783_030_400_000_i64,
                r#"{"type":"tool","tool":"context7_lookup","state":{"status":"completed","time":{"start":1000,"end":1250}}}"#,
            ),
        )
        .expect("insert part");
        drop(db);

        let service = CallAnalyticsService::new(TestPaths::new(root.to_path_buf()));
        let snapshot = service.snapshot().expect("snapshot");
        assert_eq!(snapshot.sources.len(), 3);
        assert!(snapshot
            .sources
            .iter()
            .all(|source| source.available && source.error_code.is_none()));

        let claude_mcp = find_call(
            &snapshot,
            CallAnalyticsSource::Claude,
            CallAnalyticsKind::Mcp,
            "fs/read",
        );
        assert_eq!(claude_mcp.count, 1);
        assert_eq!(claude_mcp.success_count, 1);

        let claude_skill = find_call(
            &snapshot,
            CallAnalyticsSource::Claude,
            CallAnalyticsKind::Skill,
            "briefing",
        );
        assert_eq!(claude_skill.count, 1);
        assert_eq!(claude_skill.outcome_known_count, 1);
        assert_eq!(claude_skill.success_count, 0);

        let codex_mcp = find_call(
            &snapshot,
            CallAnalyticsSource::Codex,
            CallAnalyticsKind::Mcp,
            "docs/search",
        );
        assert_eq!(codex_mcp.count, 1);
        assert_eq!(codex_mcp.duration_sample_count, 1);
        assert_eq!(codex_mcp.duration_ms_total, 1500.0);

        let codex_skill = find_call(
            &snapshot,
            CallAnalyticsSource::Codex,
            CallAnalyticsKind::Skill,
            "review",
        );
        assert_eq!(codex_skill.count, 1);

        let opencode_mcp = find_call(
            &snapshot,
            CallAnalyticsSource::OpenCode,
            CallAnalyticsKind::Mcp,
            "context7/lookup",
        );
        assert_eq!(opencode_mcp.count, 1);
        assert_eq!(opencode_mcp.duration_ms_total, 250.0);
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

    fn find_call<'a>(
        snapshot: &'a CallAnalyticsSnapshot,
        source: CallAnalyticsSource,
        kind: CallAnalyticsKind,
        name: &str,
    ) -> &'a CallAnalyticsEntry {
        snapshot
            .entries
            .iter()
            .find(|entry| entry.source == source && entry.kind == kind && entry.name == name)
            .expect("call analytics entry")
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
