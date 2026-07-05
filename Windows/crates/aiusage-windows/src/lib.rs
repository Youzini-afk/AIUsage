use std::{
    env,
    ffi::c_void,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::null_mut,
};

use aiusage_core::CredentialKind;
use aiusage_platform::{
    AppPaths, AutostartManager, BrowserProfile, BrowserSessionDiscovery, CredentialVault,
    FilePermissionGuard, PlatformError, PlatformResult, PortInspector, PortOwner, ProtectedData,
    SystemProxyReader, SystemProxySnapshot,
};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            GetLastError, GlobalFree, LocalFree, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
            ERROR_SUCCESS, HGLOBAL, HLOCAL, WIN32_ERROR,
        },
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_CLASS,
            TCP_TABLE_OWNER_PID_ALL,
        },
        Networking::{
            WinHttp::{
                WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
            },
            WinSock::AF_INET,
        },
        Security::{
            Credentials::{
                CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW,
                CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
            },
            Cryptography::{
                CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
            },
        },
        System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
            RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE,
            REG_OPTION_NON_VOLATILE, REG_SZ,
        },
    },
};

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
    fn user_home(&self) -> PlatformResult<PathBuf> {
        Self::user_profile()
    }

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

#[derive(Clone, Debug)]
pub struct WindowsProtectedData {
    description: String,
}

impl Default for WindowsProtectedData {
    fn default() -> Self {
        Self::new("AIUsage protected data")
    }
}

impl WindowsProtectedData {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
        }
    }
}

impl ProtectedData for WindowsProtectedData {
    fn protect(&self, data: &[u8]) -> PlatformResult<Vec<u8>> {
        crypt_protect(data, &self.description)
    }

    fn unprotect(&self, data: &[u8]) -> PlatformResult<Vec<u8>> {
        crypt_unprotect(data)
    }
}

#[derive(Clone, Debug)]
pub struct WindowsCredentialVault {
    target_name: String,
    protected_data: WindowsProtectedData,
}

impl WindowsCredentialVault {
    pub fn new(target_name: impl Into<String>) -> Self {
        Self {
            target_name: target_name.into(),
            protected_data: WindowsProtectedData::default(),
        }
    }

    fn target_wide(&self) -> Vec<u16> {
        wide_null(&self.target_name)
    }
}

impl Default for WindowsCredentialVault {
    fn default() -> Self {
        Self::new("com.aiusage.desktop.providerCredentials")
    }
}

impl CredentialVault for WindowsCredentialVault {
    fn load_vault(&self) -> PlatformResult<Option<Vec<u8>>> {
        let target = self.target_wide();
        let mut credential_ptr: *mut CREDENTIALW = null_mut();
        let ok = unsafe {
            CredReadW(
                PCWSTR(target.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
                &mut credential_ptr,
            )
        };
        if ok.is_err() {
            let code = unsafe { GetLastError() };
            if code.0 == 1168 {
                return Ok(None);
            }
            return Err(windows_error("CredReadW", code));
        }

        let data = unsafe {
            let credential = &*credential_ptr;
            let blob = std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec();
            CredFree(credential_ptr.cast::<c_void>());
            blob
        };
        self.protected_data.unprotect(&data).map(Some)
    }

    fn save_vault(&self, data: &[u8]) -> PlatformResult<()> {
        let protected = self.protected_data.protect(data)?;
        let target = self.target_wide();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_ptr() as *mut u16),
            CredentialBlobSize: protected.len() as u32,
            CredentialBlob: protected.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        unsafe { CredWriteW(&credential, 0) }
            .map_err(|_| windows_error("CredWriteW", unsafe { GetLastError() }))
    }

    fn delete_vault(&self) -> PlatformResult<()> {
        let target = self.target_wide();
        let result = unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) };
        if result.is_ok() {
            return Ok(());
        }
        let code = unsafe { GetLastError() };
        if code.0 == 1168 {
            return Ok(());
        }
        Err(windows_error("CredDeleteW", code))
    }

    fn supported_kinds(&self) -> Vec<CredentialKind> {
        vec![
            CredentialKind::ApiKey,
            CredentialKind::AuthFile,
            CredentialKind::Cookie,
            CredentialKind::OAuth,
            CredentialKind::Token,
            CredentialKind::WebSession,
        ]
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowsBrowserDiscovery {
    paths: WindowsAppPaths,
}

impl WindowsBrowserDiscovery {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BrowserSessionDiscovery for WindowsBrowserDiscovery {
    fn available_profiles(&self) -> PlatformResult<Vec<BrowserProfile>> {
        let mut profiles = Vec::new();
        let local = WindowsAppPaths::env_path("LOCALAPPDATA").ok();
        let roaming = WindowsAppPaths::env_path("APPDATA").ok();

        if let Some(local) = &local {
            discover_chromium_profiles(
                &mut profiles,
                "Chrome",
                local.join("Google").join("Chrome").join("User Data"),
            );
            discover_chromium_profiles(
                &mut profiles,
                "Edge",
                local.join("Microsoft").join("Edge").join("User Data"),
            );
            discover_chromium_profiles(
                &mut profiles,
                "Brave",
                local
                    .join("BraveSoftware")
                    .join("Brave-Browser")
                    .join("User Data"),
            );
        }

        if let Some(roaming) = &roaming {
            discover_chromium_profiles(
                &mut profiles,
                "Cursor",
                roaming.join("Cursor").join("User Data"),
            );
        }

        let _ = &self.paths;
        Ok(profiles)
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowsSystemProxyReader;

impl SystemProxyReader for WindowsSystemProxyReader {
    fn current_proxy(&self) -> PlatformResult<SystemProxySnapshot> {
        let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
        unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) }.map_err(|_| {
            windows_error("WinHttpGetIEProxyConfigForCurrentUser", unsafe {
                GetLastError()
            })
        })?;

        let snapshot = SystemProxySnapshot {
            http: pwstr_to_string(config.lpszProxy).and_then(|proxy| first_proxy_endpoint(&proxy)),
            https: pwstr_to_string(config.lpszProxy).and_then(|proxy| first_proxy_endpoint(&proxy)),
            socks: None,
        };

        unsafe {
            free_pwstr(config.lpszAutoConfigUrl);
            free_pwstr(config.lpszProxy);
            free_pwstr(config.lpszProxyBypass);
        }

        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowsPortInspector;

impl PortInspector for WindowsPortInspector {
    fn owner_for_port(&self, port: u16) -> PlatformResult<Option<PortOwner>> {
        let rows = tcp_owner_rows()?;
        let port_be = u16::to_be(port);
        Ok(rows
            .into_iter()
            .find(|row| row.dwLocalPort as u16 == port_be)
            .map(|row| PortOwner {
                port,
                process_id: row.dwOwningPid,
                image_path: None,
            }))
    }
}

#[derive(Clone, Debug)]
pub struct WindowsAutostartManager {
    value_name: String,
    executable_path: Option<PathBuf>,
}

impl Default for WindowsAutostartManager {
    fn default() -> Self {
        Self::new("AIUsage")
    }
}

impl WindowsAutostartManager {
    pub fn new(value_name: impl Into<String>) -> Self {
        Self {
            value_name: value_name.into(),
            executable_path: None,
        }
    }

    pub fn with_executable_path(value_name: impl Into<String>, executable_path: PathBuf) -> Self {
        Self {
            value_name: value_name.into(),
            executable_path: Some(executable_path),
        }
    }

    fn launch_command(&self) -> PlatformResult<String> {
        let path = match &self.executable_path {
            Some(path) => path.clone(),
            None => env::current_exe().map_err(PlatformError::Io)?,
        };
        Ok(format!("\"{}\"", path.display()))
    }
}

impl AutostartManager for WindowsAutostartManager {
    fn is_enabled(&self) -> PlatformResult<bool> {
        let key = open_run_key(KEY_READ)?;
        let value_name = wide_null(&self.value_name);
        let mut value_type = Default::default();
        let mut byte_len = 0_u32;
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                PCWSTR(value_name.as_ptr()),
                None,
                Some(&mut value_type),
                None,
                Some(&mut byte_len),
            )
        };
        key.close();
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        if result != ERROR_SUCCESS {
            return Err(windows_error("RegQueryValueExW", result));
        }
        Ok(byte_len > 0)
    }

    fn set_enabled(&self, enabled: bool) -> PlatformResult<()> {
        let key = create_run_key()?;
        let value_name = wide_null(&self.value_name);
        let result = if enabled {
            let command = wide_null(&self.launch_command()?);
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    command.as_ptr().cast::<u8>(),
                    command.len() * std::mem::size_of::<u16>(),
                )
            };
            unsafe {
                RegSetValueExW(
                    key.0,
                    PCWSTR(value_name.as_ptr()),
                    None,
                    REG_SZ,
                    Some(bytes),
                )
            }
        } else {
            unsafe { RegDeleteValueW(key.0, PCWSTR(value_name.as_ptr())) }
        };
        key.close();

        if !enabled && result == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        if result != ERROR_SUCCESS {
            return Err(windows_error(
                if enabled {
                    "RegSetValueExW"
                } else {
                    "RegDeleteValueW"
                },
                result,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowsFilePermissionGuard;

impl FilePermissionGuard for WindowsFilePermissionGuard {
    fn restrict_current_user(&self, path: &Path) -> PlatformResult<()> {
        if !path.exists() {
            return Ok(());
        }

        let account =
            current_user_account().ok_or(PlatformError::MissingPath("USERDOMAIN/USERNAME"))?;
        let status = Command::new("icacls")
            .arg(path.as_os_str())
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{account}:F"))
            .arg("/grant:r")
            .arg("*S-1-5-18:F")
            .arg("/grant:r")
            .arg("*S-1-5-32-544:F")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(PlatformError::InvalidData("icacls failed"))
        }
    }
}

fn discover_chromium_profiles(
    profiles: &mut Vec<BrowserProfile>,
    browser_name: &str,
    base: PathBuf,
) {
    if !base.exists() {
        return;
    }

    let mut candidates = vec!["Default".to_string()];
    candidates.extend((1..=20).map(|index| format!("Profile {index}")));
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !candidates.contains(&name) {
                    candidates.push(name);
                }
            }
        }
    }

    for profile_name in candidates {
        if let Some(cookies_db_path) = chromium_cookie_path(&base, &profile_name) {
            profiles.push(BrowserProfile {
                browser_name: browser_name.to_string(),
                profile_name,
                cookies_db_path,
            });
        }
    }
}

fn chromium_cookie_path(base: &Path, profile_name: &str) -> Option<PathBuf> {
    let profile = base.join(profile_name);
    [
        profile.join("Network").join("Cookies"),
        profile.join("Cookies"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn tcp_owner_rows() -> PlatformResult<Vec<MIB_TCPROW_OWNER_PID>> {
    let mut size = 0u32;
    let class = TCP_TABLE_CLASS(TCP_TABLE_OWNER_PID_ALL.0);
    let family = AF_INET.0 as u32;
    let first = unsafe { GetExtendedTcpTable(None, &mut size, false, family, class, 0) };
    if first != ERROR_INSUFFICIENT_BUFFER.0 {
        return Err(windows_error(
            "GetExtendedTcpTable(size)",
            WIN32_ERROR(first),
        ));
    }

    let mut buffer = vec![0u8; size as usize];
    let result = unsafe {
        GetExtendedTcpTable(
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            &mut size,
            false,
            family,
            class,
            0,
        )
    };
    if result != 0 {
        return Err(windows_error("GetExtendedTcpTable", WIN32_ERROR(result)));
    }

    let table = unsafe { &*(buffer.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>()) };
    let count = table.dwNumEntries as usize;
    let first_row = table.table.as_ptr();
    Ok((0..count)
        .map(|index| unsafe { *first_row.add(index) })
        .collect())
}

struct RegistryKey(HKEY);

impl RegistryKey {
    fn close(self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open_run_key(
    access: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> PlatformResult<RegistryKey> {
    let subkey = wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let mut key = HKEY::default();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            access,
            &mut key,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(windows_error("RegOpenKeyExW", result));
    }
    Ok(RegistryKey(key))
}

fn create_run_key() -> PlatformResult<RegistryKey> {
    let subkey = wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let mut key = HKEY::default();
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(windows_error("RegCreateKeyExW", result));
    }
    Ok(RegistryKey(key))
}

fn crypt_protect(data: &[u8], description: &str) -> PlatformResult<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let description = wide_null(description);
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR(description.as_ptr()),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| windows_error("CryptProtectData", unsafe { GetLastError() }))?;
    data_blob_to_vec(output)
}

fn crypt_unprotect(data: &[u8]) -> PlatformResult<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| windows_error("CryptUnprotectData", unsafe { GetLastError() }))?;
    data_blob_to_vec(output)
}

fn data_blob_to_vec(blob: CRYPT_INTEGER_BLOB) -> PlatformResult<Vec<u8>> {
    if blob.pbData.is_null() {
        return Err(PlatformError::InvalidData("empty CRYPT_INTEGER_BLOB"));
    }
    let data = unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec() };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(blob.pbData.cast::<c_void>())));
    }
    Ok(data)
}

fn pwstr_to_string(value: PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe { value.to_string().ok() }.filter(|value| !value.trim().is_empty())
}

unsafe fn free_pwstr(value: PWSTR) {
    if !value.is_null() {
        let _ = GlobalFree(Some(HGLOBAL(value.as_ptr().cast::<c_void>())));
    }
}

fn first_proxy_endpoint(proxy: &str) -> Option<String> {
    proxy
        .split(';')
        .map(str::trim)
        .find(|part| !part.is_empty())
        .map(|part| {
            part.split_once('=')
                .map(|(_, endpoint)| endpoint.to_string())
                .unwrap_or_else(|| part.to_string())
        })
}

fn current_user_account() -> Option<String> {
    let username = env::var("USERNAME").ok()?.trim().to_string();
    if username.is_empty() {
        return None;
    }
    let domain = env::var("USERDOMAIN").unwrap_or_default();
    let domain = domain.trim();
    if domain.is_empty() {
        Some(username)
    } else {
        Some(format!("{domain}\\{username}"))
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn windows_error(operation: &'static str, code: WIN32_ERROR) -> PlatformError {
    PlatformError::WindowsApi {
        operation,
        code: code.0 as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aiusage_platform::{
        AppPaths, AutostartManager, BrowserSessionDiscovery, CredentialVault, FilePermissionGuard,
        PortInspector, ProtectedData, SystemProxyReader,
    };
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

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

    #[test]
    fn dpapi_round_trips_local_data() {
        let protected = WindowsProtectedData::new("AIUsage test data");
        let encrypted = protected
            .protect(b"phase-b-platform")
            .expect("DPAPI protect should succeed");
        assert_ne!(encrypted, b"phase-b-platform");
        let plain = protected
            .unprotect(&encrypted)
            .expect("DPAPI unprotect should succeed");
        assert_eq!(plain, b"phase-b-platform");
    }

    #[test]
    fn credential_vault_round_trips_and_deletes() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let vault = WindowsCredentialVault::new(format!("com.aiusage.test.{suffix}"));
        vault.delete_vault().expect("delete should be idempotent");
        vault
            .save_vault(b"{\"phase\":\"b\"}")
            .expect("credential write should succeed");
        assert_eq!(
            vault.load_vault().expect("credential read should succeed"),
            Some(b"{\"phase\":\"b\"}".to_vec())
        );
        vault
            .delete_vault()
            .expect("credential delete should succeed");
        assert_eq!(vault.load_vault().expect("missing credential is ok"), None);
    }

    #[test]
    fn browser_discovery_is_non_fatal() {
        let discovery = WindowsBrowserDiscovery::new();
        let profiles = discovery
            .available_profiles()
            .expect("browser discovery should not fail when paths are absent");
        for profile in profiles {
            assert!(profile.cookies_db_path.ends_with("Cookies"));
        }
    }

    #[test]
    fn system_proxy_reader_is_non_fatal() {
        let reader = WindowsSystemProxyReader;
        let snapshot = reader
            .current_proxy()
            .expect("system proxy API should return a snapshot");
        let _ = snapshot.is_any_enabled();
    }

    #[test]
    fn port_inspector_accepts_unused_port_lookup() {
        let inspector = WindowsPortInspector;
        let _ = inspector
            .owner_for_port(9)
            .expect("TCP table lookup should succeed");
    }

    #[test]
    fn file_permission_guard_restricts_temp_file() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("aiusage-permission-{suffix}.txt"));
        fs::write(&path, b"secret").expect("temp file write should succeed");
        let guard = WindowsFilePermissionGuard;
        guard
            .restrict_current_user(&path)
            .expect("icacls should restrict the temp file");
        fs::remove_file(path).expect("restricted file should remain removable by current user");
    }

    #[test]
    fn autostart_manager_round_trips_unique_run_value() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let manager = WindowsAutostartManager::with_executable_path(
            format!("AIUsageTest{suffix}"),
            env::current_exe().expect("test executable path"),
        );
        manager
            .set_enabled(false)
            .expect("autostart delete should be idempotent");
        assert!(!manager.is_enabled().expect("autostart should be disabled"));
        manager
            .set_enabled(true)
            .expect("autostart enable should write HKCU Run");
        assert!(manager.is_enabled().expect("autostart should be enabled"));
        manager
            .set_enabled(false)
            .expect("autostart disable should delete HKCU Run value");
        assert!(!manager.is_enabled().expect("autostart should be disabled"));
    }
}
