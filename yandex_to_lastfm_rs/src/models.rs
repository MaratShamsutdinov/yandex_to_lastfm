use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct PlaybackState {
    pub last_track: Option<String>,
    pub last_artist: Option<String>,
    pub last_title: Option<String>,
    pub last_album: Option<String>,
    pub started_at: i64,
    pub scrobbled: bool,

    pub last_scrobble_error: Option<String>,
    pub last_scrobble_error_at: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ExtensionRuntimeState {
    pub last_seen_at: Option<i64>,
    pub connected: bool,

    pub yandex_tab_open: bool,
    pub metadata_active: bool,
    pub reload_likely_needed: bool,

    pub reload_popup_shown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingScrobble {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub timestamp: i64,
    pub duration: Option<f64>,
    pub queued_at: i64,
    pub retry_count: u32,
}

#[derive(Debug, Default)]
pub struct LastfmRuntimeState {
    pub connected: bool,
    pub last_error: Option<String>,
    pub error_popup_shown: bool,

    pub watchdog_fail_count: u32,
    pub last_success_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct IncomingTrack {
    pub schema_version: Option<u32>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub event_type: Option<String>,
    pub event_id: Option<String>,
    pub sent_at: Option<i64>,
    pub page_url: Option<String>,
    pub page_ts: Option<i64>,
    pub reason: Option<String>,

    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub cover_url: Option<String>,
    pub duration: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ExtensionPingRequest {
    pub schema_version: Option<u32>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub sent_at: Option<i64>,

    pub yandex_tab_open: Option<bool>,
    pub metadata_active: Option<bool>,
    pub reload_likely_needed: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub app: &'static str,
    pub version: &'static str,
    pub extension_connected: bool,
    pub extension_last_seen_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ExtensionPingResponse {
    pub ok: bool,
    pub app: &'static str,
    pub version: &'static str,
    pub extension_connected: bool,
    pub extension_last_seen_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TrackAcceptedResponse {
    pub ok: bool,
    pub accepted: bool,
    pub new_track: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PopupKind {
    Track,
    StartupOk,
    StartupError,
    ExtensionMissing,
    ReloadNeeded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopupPayload {
    pub kind: PopupKind,
    pub title: String,
    pub line1: String,
    pub line2: String,
    pub footer: String,
    pub cover_url: Option<String>,
    pub cover_path: Option<String>,
    pub dominant_rgb: Option<[u8; 3]>,
}
