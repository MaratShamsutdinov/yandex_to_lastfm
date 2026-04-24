use crate::config::{LASTFM_API_KEY, LASTFM_API_SECRET};

use serde::{Deserialize, Serialize};

use std::fs;
use std::path::PathBuf;

const APP_CONFIG_DIR_NAME: &str = "yamusic_lastfm_popup";
const APP_CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LastfmConfig {
    pub api_key: String,
    pub api_secret: String,
    pub username: String,
    pub password: String,

    #[serde(default)]
    pub session_key: String,

    #[serde(default)]
    pub synced_from_extension: bool,

    #[serde(default)]
    pub auth_token: String,

    #[serde(default)]
    pub auth_token_requested_at: i64,
}

impl LastfmConfig {
    pub fn has_full_credentials(&self) -> bool {
        !self.api_key.trim().is_empty()
            && !self.api_secret.trim().is_empty()
            && !self.username.trim().is_empty()
            && !self.password.trim().is_empty()
    }

    pub fn has_companion_auth(&self) -> bool {
        !self.api_key.trim().is_empty()
            && !self.api_secret.trim().is_empty()
            && !self.session_key.trim().is_empty()
    }

    pub fn is_complete(&self) -> bool {
        self.has_full_credentials() || self.has_companion_auth()
    }

    pub fn normalized(&self) -> Self {
        let api_key = if self.api_key.trim().is_empty() {
            LASTFM_API_KEY.to_string()
        } else {
            self.api_key.trim().to_string()
        };

        let api_secret = if self.api_secret.trim().is_empty() {
            LASTFM_API_SECRET.to_string()
        } else {
            self.api_secret.trim().to_string()
        };

        Self {
            api_key,
            api_secret,
            username: self.username.trim().to_string(),
            password: self.password.trim().to_string(),
            session_key: self.session_key.trim().to_string(),
            synced_from_extension: self.synced_from_extension,
            auth_token: self.auth_token.trim().to_string(),
            auth_token_requested_at: self.auth_token_requested_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub lastfm: LastfmConfig,

    #[serde(default)]
    pub launch_on_startup: bool,
}

impl AppConfig {
    pub fn is_complete(&self) -> bool {
        self.lastfm.is_complete()
    }

    pub fn has_companion_auth(&self) -> bool {
        self.lastfm.has_companion_auth()
    }

    pub fn normalized(&self) -> Self {
        Self {
            lastfm: self.lastfm.normalized(),
            launch_on_startup: self.launch_on_startup,
        }
    }
}

pub fn default_app_config() -> AppConfig {
    AppConfig::default()
}

pub fn config_dir() -> Result<PathBuf, String> {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let mut dir = PathBuf::from(appdata);
        dir.push(APP_CONFIG_DIR_NAME);
        return Ok(dir);
    }

    if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
        let mut dir = PathBuf::from(local_appdata);
        dir.push(APP_CONFIG_DIR_NAME);
        return Ok(dir);
    }

    let exe_path = std::env::current_exe().map_err(|e| format!("current_exe error: {e}"))?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "cannot resolve executable directory".to_string())?;

    let mut dir = exe_dir.to_path_buf();
    dir.push(APP_CONFIG_DIR_NAME);
    Ok(dir)
}

pub fn config_path() -> Result<PathBuf, String> {
    let mut path = config_dir()?;
    path.push(APP_CONFIG_FILE_NAME);
    Ok(path)
}

pub fn pending_scrobbles_path() -> Result<PathBuf, String> {
    let mut path = config_dir()?;
    path.push("pending_scrobbles.json");
    Ok(path)
}

pub fn load_app_config() -> Result<Option<AppConfig>, String> {
    let path = config_path()?;

    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("config read error ({}): {e}", path.display()))?;

    let parsed = serde_json::from_str::<AppConfig>(&raw)
        .map_err(|e| format!("config parse error ({}): {e}", path.display()))?;

    Ok(Some(parsed.normalized()))
}

pub fn save_app_config(config: &AppConfig) -> Result<(), String> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("config dir create error ({}): {e}", dir.display()))?;

    let path = config_path()?;
    let normalized = config.normalized();

    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("config serialize error: {e}"))?;

    fs::write(&path, json).map_err(|e| format!("config write error ({}): {e}", path.display()))
}

pub fn clear_app_config() -> Result<(), String> {
    let path = config_path()?;

    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| format!("config remove error ({}): {e}", path.display()))?;
    }

    Ok(())
}
