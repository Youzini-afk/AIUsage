use std::path::PathBuf;

use aiusage_core::{CredentialKind, ProxyTrack};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("required path is unavailable: {0}")]
    MissingPath(&'static str),
    #[error("Windows API call failed: {operation} ({code})")]
    WindowsApi { operation: &'static str, code: i32 },
    #[error("operation is not implemented for this platform phase: {0}")]
    NotImplemented(&'static str),
    #[error("platform data was invalid: {0}")]
    InvalidData(&'static str),
    #[error("platform IO failed: {0}")]
    Io(#[from] std::io::Error),
}

pub type PlatformResult<T> = Result<T, PlatformError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserProfile {
    pub browser_name: String,
    pub profile_name: String,
    pub cookies_db_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortOwner {
    pub port: u16,
    pub process_id: u32,
    pub image_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemProxySnapshot {
    pub http: Option<String>,
    pub https: Option<String>,
    pub socks: Option<String>,
}

impl SystemProxySnapshot {
    pub fn is_any_enabled(&self) -> bool {
        self.http.is_some() || self.https.is_some() || self.socks.is_some()
    }
}

pub trait AppPaths {
    fn user_home(&self) -> PlatformResult<PathBuf>;
    fn app_config_dir(&self) -> PlatformResult<PathBuf>;
    fn app_data_dir(&self) -> PlatformResult<PathBuf>;
    fn app_cache_dir(&self) -> PlatformResult<PathBuf>;
    fn codex_home(&self) -> PlatformResult<PathBuf>;
    fn claude_home(&self) -> PlatformResult<PathBuf>;
    fn opencode_config_dir(&self) -> PlatformResult<PathBuf>;
}

pub trait CredentialVault {
    fn load_vault(&self) -> PlatformResult<Option<Vec<u8>>>;
    fn save_vault(&self, data: &[u8]) -> PlatformResult<()>;
    fn delete_vault(&self) -> PlatformResult<()>;
    fn supported_kinds(&self) -> Vec<CredentialKind>;
}

pub trait ProtectedData {
    fn protect(&self, data: &[u8]) -> PlatformResult<Vec<u8>>;
    fn unprotect(&self, data: &[u8]) -> PlatformResult<Vec<u8>>;
}

pub trait BrowserSessionDiscovery {
    fn available_profiles(&self) -> PlatformResult<Vec<BrowserProfile>>;
}

pub trait PortInspector {
    fn owner_for_port(&self, port: u16) -> PlatformResult<Option<PortOwner>>;
}

pub trait SystemProxyReader {
    fn current_proxy(&self) -> PlatformResult<SystemProxySnapshot>;
}

pub trait FilePermissionGuard {
    fn restrict_current_user(&self, path: &std::path::Path) -> PlatformResult<()>;
}

pub trait AutostartManager {
    fn is_enabled(&self) -> PlatformResult<bool>;
    fn set_enabled(&self, enabled: bool) -> PlatformResult<()>;
}

pub trait CertificateTrustStore {
    fn is_certificate_trusted(&self, sha256_thumbprint: &str) -> PlatformResult<bool>;
    fn trust_certificate_der(&self, certificate_der: &[u8]) -> PlatformResult<()>;
}

pub trait ProxySupervisor {
    fn is_track_running(&self, track: ProxyTrack) -> PlatformResult<bool>;
}
