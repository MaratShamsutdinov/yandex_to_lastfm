use crate::app_config::{
    load_app_config, pending_scrobbles_path, save_app_config, AppConfig, LastfmConfig,
};
use crate::config::{
    APP_FOOTER_TEXT, COVER_DOWNLOAD_TIMEOUT_MS, DOMINANT_MIN_BRIGHTNESS, DOMINANT_MIN_SATURATION,
    DOMINANT_SAMPLE_GRID, SCROBBLE_AFTER_SECS, SERVER_BIND_ADDR,
};
use crate::lastfm::{
    get_auth_token, get_session_key, get_session_key_from_token, scrobble, scrobble_batch,
};
use crate::models::{
    ExtensionPingRequest, ExtensionPingResponse, ExtensionRuntimeState, HealthResponse,
    IncomingTrack, LastfmRuntimeState, PendingScrobble, PlaybackState, PopupKind, PopupPayload,
    TrackAcceptedResponse,
};
use serde::{Deserialize, Serialize};

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use std::fs;
use std::io::Write;
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::sleep;

pub type PopupNotifier = Arc<dyn Fn(PopupPayload) + Send + Sync + 'static>;
pub type ExtensionStatusNotifier = Arc<dyn Fn(bool) + Send + Sync + 'static>;

const EXTENSION_STALE_SECS: i64 = 30;
const PENDING_BATCH_SIZE: usize = 20;

const LASTFM_WATCHDOG_FAIL_THRESHOLD: u32 = 3;
const LASTFM_WATCHDOG_SUCCESS_GRACE_SECS: i64 = 45;

static SERVER_STATE_HANDLE: OnceLock<ServerState> = OnceLock::new();

#[derive(Clone)]
pub struct ServerState {
    pub client: Client,
    pub lastfm_config: Arc<Mutex<LastfmConfig>>,
    pub session_key: Arc<Mutex<Option<String>>>,
    pub playback: Arc<Mutex<PlaybackState>>,
    pub extension: Arc<Mutex<ExtensionRuntimeState>>,
    pub lastfm_runtime: Arc<Mutex<LastfmRuntimeState>>,
    pub popup_notifier: PopupNotifier,
    pub extension_status_notifier: ExtensionStatusNotifier,
}

pub async fn start_lastfm_browser_auth(
    lastfm_config: &LastfmConfig,
) -> Result<(String, String), String> {
    let normalized = lastfm_config.normalized();

    if normalized.api_key.is_empty() || normalized.api_secret.is_empty() {
        return Err("Last.fm API Key / API Secret missing".to_string());
    }

    let client = Client::builder()
        .build()
        .map_err(|e| format!("reqwest client error: {e}"))?;

    let token = get_auth_token(&client, &normalized).await?;
    println!("[LASTFM AUTH] TOKEN RECEIVED: {}", token);

    let auth_url = format!(
        "https://www.last.fm/api/auth/?api_key={}&token={}",
        normalized.api_key, token
    );

    if Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &auth_url])
        .spawn()
        .is_err()
    {
        return Err(format!(
            "Could not open browser automatically.\n\nOpen this link manually:\n{}",
            auth_url
        ));
    }

    Ok((token, auth_url))
}

pub async fn finish_lastfm_browser_auth(
    lastfm_config: &LastfmConfig,
    token: &str,
) -> Result<String, String> {
    let normalized = lastfm_config.normalized();

    if normalized.api_key.is_empty() || normalized.api_secret.is_empty() {
        return Err("Last.fm API Key / API Secret missing".to_string());
    }

    println!("[LASTFM AUTH] TOKEN USED FOR SESSION: {}", token);

    get_session_key_from_token(
        &Client::builder()
            .build()
            .map_err(|e| format!("reqwest client error: {e}"))?,
        &normalized,
        token,
    )
    .await
}

pub async fn apply_lastfm_config_hot(config: &AppConfig) -> Result<(), String> {
    let Some(state) = SERVER_STATE_HANDLE.get().cloned() else {
        return Err("Server state handle is not available yet".to_string());
    };

    let normalized = config.lastfm.normalized();

    {
        let mut cfg = state.lastfm_config.lock().await;
        *cfg = normalized.clone();
    }

    {
        let mut sk = state.session_key.lock().await;
        *sk = if normalized.session_key.trim().is_empty() {
            None
        } else {
            Some(normalized.session_key.trim().to_string())
        };
    }

    {
        let mut lf = state.lastfm_runtime.lock().await;

        if normalized.session_key.trim().is_empty() {
            lf.connected = false;
            lf.last_error = Some("Last.fm session key missing".to_string());
        } else {
            lf.connected = true;
            lf.last_error = None;
            lf.error_popup_shown = false;
            lf.watchdog_fail_count = 0;
            lf.last_success_at = Some(unix_ts());
        }
    }

    match flush_pending_scrobbles(&state).await {
        Ok(n) if n > 0 => {
            println!("[PENDING_SCROBBLES] hot-applied and flushed {}", n);
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("[PENDING_SCROBBLES] hot-apply flush error: {}", e);
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompanionImportLastfmRequest {
    pub schema_version: Option<u32>,
    pub source: Option<String>,
    pub synced_at: Option<i64>,

    pub api_key: String,
    pub api_secret: String,

    #[serde(default)]
    pub username: String,

    pub session_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompanionImportLastfmResponse {
    pub ok: bool,
    pub imported: bool,
    pub source: String,
    pub username: String,
    pub has_session_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompanionExportLastfmResponse {
    pub ok: bool,
    pub source: String,
    pub api_key: String,
    pub api_secret: String,
    pub username: String,
    pub session_key: String,
    pub synced_from_extension: bool,
}

fn pending_scrobble_key(item: &PendingScrobble) -> String {
    format!("{}|{}|{}", item.artist, item.track, item.timestamp)
}

fn load_pending_scrobbles() -> Result<Vec<PendingScrobble>, String> {
    let path = pending_scrobbles_path()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("pending scrobbles read error ({}): {e}", path.display()))?;

    match serde_json::from_str::<Vec<PendingScrobble>>(&raw) {
        Ok(parsed) => Ok(parsed),
        Err(parse_err) => {
            let quarantine_path = path.with_extension(format!("json.corrupt.{}", unix_ts()));

            fs::rename(&path, &quarantine_path).map_err(|rename_err| {
                format!(
                    "pending scrobbles parse error ({}): {}; quarantine rename error ({}): {}",
                    path.display(),
                    parse_err,
                    quarantine_path.display(),
                    rename_err
                )
            })?;

            eprintln!(
                "[PENDING_SCROBBLES] corrupt file quarantined: '{}' -> '{}'; parse error: {}",
                path.display(),
                quarantine_path.display(),
                parse_err
            );

            Ok(Vec::new())
        }
    }
}

fn save_pending_scrobbles(items: &[PendingScrobble]) -> Result<(), String> {
    let path = pending_scrobbles_path()?;
    let dir = path
        .parent()
        .ok_or_else(|| "pending scrobbles dir resolve error".to_string())?;

    fs::create_dir_all(dir).map_err(|e| {
        format!(
            "pending scrobbles dir create error ({}): {e}",
            dir.display()
        )
    })?;

    let json = serde_json::to_string_pretty(items)
        .map_err(|e| format!("pending scrobbles serialize error: {e}"))?;

    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let temp_path = path.with_extension(format!("json.tmp.{}.{}", std::process::id(), ts_ms));

    {
        let mut file = fs::File::create(&temp_path).map_err(|e| {
            format!(
                "pending scrobbles temp create error ({}): {e}",
                temp_path.display()
            )
        })?;

        file.write_all(json.as_bytes()).map_err(|e| {
            format!(
                "pending scrobbles temp write error ({}): {e}",
                temp_path.display()
            )
        })?;

        file.sync_all().map_err(|e| {
            format!(
                "pending scrobbles temp sync error ({}): {e}",
                temp_path.display()
            )
        })?;
    }

    match fs::rename(&temp_path, &path) {
        Ok(()) => Ok(()),
        Err(first_rename_err) => {
            if path.exists() {
                fs::remove_file(&path).map_err(|remove_err| {
                    let _ = fs::remove_file(&temp_path);

                    format!(
                        "pending scrobbles replace error ({}): rename error: {}; remove existing error: {}",
                        path.display(),
                        first_rename_err,
                        remove_err
                    )
                })?;

                fs::rename(&temp_path, &path).map_err(|second_rename_err| {
                    let _ = fs::remove_file(&temp_path);

                    format!(
                        "pending scrobbles rename error ({} -> {}): first error: {}; second error: {}",
                        temp_path.display(),
                        path.display(),
                        first_rename_err,
                        second_rename_err
                    )
                })?;

                Ok(())
            } else {
                let _ = fs::remove_file(&temp_path);

                Err(format!(
                    "pending scrobbles rename error ({} -> {}): {}",
                    temp_path.display(),
                    path.display(),
                    first_rename_err
                ))
            }
        }
    }
}

fn enqueue_pending_scrobble(item: PendingScrobble) -> Result<(), String> {
    let mut items = load_pending_scrobbles()?;
    let key = pending_scrobble_key(&item);

    if items.iter().any(|x| pending_scrobble_key(x) == key) {
        return Ok(());
    }

    items.push(item);
    items.sort_by_key(|x| x.timestamp);

    if items.len() > 500 {
        let keep_from = items.len().saturating_sub(500);
        items = items.split_off(keep_from);
    }

    save_pending_scrobbles(&items)
}

async fn flush_pending_scrobbles(state: &ServerState) -> Result<usize, String> {
    let mut items = load_pending_scrobbles()?;
    if items.is_empty() {
        return Ok(0);
    }

    items.sort_by_key(|x| x.timestamp);

    let take_n = items.len().min(PENDING_BATCH_SIZE);
    let batch: Vec<PendingScrobble> = items.iter().take(take_n).cloned().collect();

    let session_key = {
        let sk = state.session_key.lock().await;
        sk.clone()
    };

    let Some(session_key) = session_key else {
        return Ok(0);
    };

    let cfg = {
        let cfg = state.lastfm_config.lock().await;
        cfg.clone()
    };

    let resp = scrobble_batch(&state.client, &cfg, &session_key, &batch).await?;

    println!("[PENDING_SCROBBLES] flushed={} resp={}", batch.len(), resp);

    items.drain(0..take_n);
    save_pending_scrobbles(&items)?;

    Ok(batch.len())
}

async fn mark_extension_seen(state: &ServerState, source: &str) {
    let now = unix_ts();
    let mut became_connected = false;

    {
        let mut ext = state.extension.lock().await;
        ext.last_seen_at = Some(now);

        if !ext.connected {
            ext.connected = true;
            became_connected = true;
        }
    }

    if became_connected {
        println!("[EXTENSION] connected via {}", source);
        (state.extension_status_notifier)(true);
    }
}

pub async fn scrobble_worker(state: ServerState) {
    loop {
        sleep(Duration::from_secs(1)).await;

        let maybe_scrobble = {
            let mut pb = state.playback.lock().await;
            let now = unix_ts();

            if pb.scrobbled {
                None
            } else if pb.started_at == 0 {
                None
            } else {
                let elapsed = now - pb.started_at;

                if elapsed >= SCROBBLE_AFTER_SECS {
                    let artist = pb.last_artist.clone();
                    let track = pb.last_title.clone();
                    let started_at = pb.started_at;

                    match (artist, track) {
                        (Some(artist), Some(track)) => {
                            println!(
                                "[SCROBBLE_WORKER] ready => artist='{}' track='{}' elapsed={}s",
                                artist, track, elapsed
                            );

                            pb.scrobbled = true;

                            Some((artist, track, started_at))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
        };

        let Some((artist, track, started_at)) = maybe_scrobble else {
            continue;
        };

        if let Ok(n) = flush_pending_scrobbles(&state).await {
            if n > 0 {
                println!("[PENDING_SCROBBLES] pre-flush before live scrobble: {}", n);
            }
        }

        let session_key = {
            let sk = state.session_key.lock().await;
            sk.clone()
        };

        let Some(session_key) = session_key else {
            let pending_item = PendingScrobble {
                artist: artist.clone(),
                track: track.clone(),
                album: {
                    let pb = state.playback.lock().await;
                    pb.last_album.clone()
                },
                timestamp: started_at,
                duration: None,
                queued_at: unix_ts(),
                retry_count: 0,
            };

            if let Err(enq_err) = enqueue_pending_scrobble(pending_item) {
                eprintln!(
                    "[PENDING_SCROBBLES] enqueue error before session key: {}",
                    enq_err
                );
            } else {
                println!("[PENDING_SCROBBLES] queued before session key");
            }

            continue;
        };

        let cfg = {
            let cfg = state.lastfm_config.lock().await;
            cfg.clone()
        };

        match scrobble(
            &state.client,
            &cfg,
            &session_key,
            &artist,
            &track,
            started_at,
        )
        .await
        {
            Ok(v) => {
                println!(
                    "[SCROBBLE_WORKER] SCROBBLE OK => artist='{}' track='{}' resp={}",
                    artist, track, v
                );

                let mut lf = state.lastfm_runtime.lock().await;
                lf.connected = true;
                lf.last_error = None;
                lf.error_popup_shown = false;
                lf.watchdog_fail_count = 0;
                lf.last_success_at = Some(unix_ts());
            }
            Err(e) => {
                eprintln!(
                    "[SCROBBLE_WORKER] SCROBBLE ERROR => artist='{}' track='{}' err={}",
                    artist, track, e
                );

                let album = {
                    let pb = state.playback.lock().await;
                    pb.last_album.clone()
                };

                let pending_item = PendingScrobble {
                    artist: artist.clone(),
                    track: track.clone(),
                    album,
                    timestamp: started_at,
                    duration: None,
                    queued_at: unix_ts(),
                    retry_count: 0,
                };

                if let Err(enq_err) = enqueue_pending_scrobble(pending_item) {
                    eprintln!("[PENDING_SCROBBLES] enqueue error: {}", enq_err);
                }

                {
                    let mut lf = state.lastfm_runtime.lock().await;
                    lf.connected = false;
                    lf.last_error = Some(e.clone());
                }

                let mut show_popup = false;

                {
                    let mut pb = state.playback.lock().await;
                    let current = format!("{artist} - {track}");
                    let now = unix_ts();

                    if pb.last_track.as_deref() == Some(current.as_str()) {
                        let should_retry_now = pb
                            .last_scrobble_error_at
                            .map(|ts| now - ts >= 10)
                            .unwrap_or(true);

                        if should_retry_now {
                            pb.scrobbled = false;
                            pb.last_scrobble_error_at = Some(now);
                        } else {
                            pb.scrobbled = true;
                        }

                        let is_new_error = pb.last_scrobble_error.as_deref() != Some(e.as_str());
                        if is_new_error {
                            pb.last_scrobble_error = Some(e.clone());
                            show_popup = true;
                        }
                    }
                }

                if show_popup {
                    (state.popup_notifier)(build_lastfm_runtime_error_popup(&e));
                }
            }
        }
    }
}

pub async fn extension_watchdog_worker(state: ServerState) {
    loop {
        sleep(Duration::from_secs(2)).await;

        let mut became_disconnected = false;

        {
            let now = unix_ts();
            let mut ext = state.extension.lock().await;

            let still_connected = ext
                .last_seen_at
                .map(|ts| now - ts <= EXTENSION_STALE_SECS)
                .unwrap_or(false);

            if ext.connected && !still_connected {
                ext.connected = false;
                ext.reload_popup_shown = false;
                became_disconnected = true;
            }
        }

        if became_disconnected {
            println!("[EXTENSION] heartbeat timed out");

            (state.popup_notifier)(build_extension_missing_popup());
            (state.extension_status_notifier)(false);
        }
    }
}

pub async fn initial_extension_popup_worker(state: ServerState) {
    sleep(Duration::from_secs(8)).await;

    let connected = {
        let ext = state.extension.lock().await;
        ext.connected
    };

    if !connected {
        println!("[EXTENSION] startup popup requested");
        (state.popup_notifier)(build_extension_missing_popup());
    }
}

pub async fn lastfm_watchdog_worker(state: ServerState) {
    sleep(Duration::from_secs(12)).await;

    loop {
        sleep(Duration::from_secs(10)).await;

        let has_session_key = {
            let sk = state.session_key.lock().await;
            sk.is_some()
        };

        let should_refresh = {
            let lf = state.lastfm_runtime.lock().await;
            !has_session_key || !lf.connected
        };

        if !should_refresh {
            match flush_pending_scrobbles(&state).await {
                Ok(n) if n > 0 => {
                    println!("[PENDING_SCROBBLES] sent {} cached scrobbles", n);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[PENDING_SCROBBLES] flush error: {}", e);
                }
            }

            continue;
        }

        let cfg = {
            let cfg = state.lastfm_config.lock().await;
            cfg.clone()
        };

        if cfg.has_companion_auth() && !cfg.has_full_credentials() {
            {
                let mut session_key = state.session_key.lock().await;
                if session_key.is_none() {
                    *session_key = Some(cfg.session_key.clone());
                }
            }

            {
                let mut lf = state.lastfm_runtime.lock().await;
                lf.connected = true;
                lf.last_error = None;
                lf.error_popup_shown = false;
                lf.watchdog_fail_count = 0;
                lf.last_success_at = Some(unix_ts());
            }

            match flush_pending_scrobbles(&state).await {
                Ok(n) if n > 0 => {
                    println!("[PENDING_SCROBBLES] sent {} cached scrobbles", n);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[PENDING_SCROBBLES] flush error: {}", e);
                }
            }

            continue;
        }

        if !cfg.has_full_credentials() && !cfg.has_companion_auth() {
            eprintln!("[LASTFM] watchdog waiting for manual config or extension sync...");
            continue;
        }

        println!("Получаем session key Last.fm...");

        match get_session_key(&state.client, &cfg).await {
            Ok(new_session_key) => {
                let mut show_connected_popup = false;

                {
                    let mut session_key = state.session_key.lock().await;
                    let had_session = session_key.is_some();
                    *session_key = Some(new_session_key);

                    let mut lf = state.lastfm_runtime.lock().await;
                    if !lf.connected || !had_session {
                        show_connected_popup = true;
                    }

                    lf.connected = true;
                    lf.last_error = None;
                    lf.error_popup_shown = false;
                    lf.watchdog_fail_count = 0;
                    lf.last_success_at = Some(unix_ts());
                }

                if show_connected_popup {
                    println!("[LASTFM] connection established");
                    (state.popup_notifier)(build_startup_ok_popup());
                }

                match flush_pending_scrobbles(&state).await {
                    Ok(n) if n > 0 => {
                        println!("[PENDING_SCROBBLES] sent {} cached scrobbles", n);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[PENDING_SCROBBLES] flush error: {}", e);
                    }
                }
            }
            Err(e) => {
                let mut show_error_popup = false;
                let mut should_mark_disconnected = false;

                {
                    let now = unix_ts();
                    let mut lf = state.lastfm_runtime.lock().await;

                    let previous_error = lf.last_error.clone();
                    lf.watchdog_fail_count += 1;
                    lf.last_error = Some(e.clone());

                    let recent_success = lf
                        .last_success_at
                        .map(|ts| now - ts <= LASTFM_WATCHDOG_SUCCESS_GRACE_SECS)
                        .unwrap_or(false);

                    if lf.watchdog_fail_count >= LASTFM_WATCHDOG_FAIL_THRESHOLD && !recent_success {
                        should_mark_disconnected = true;
                    }

                    if should_mark_disconnected {
                        let was_connected = lf.connected;
                        let error_changed = previous_error.as_deref() != Some(e.as_str());

                        lf.connected = false;

                        if was_connected || error_changed || !lf.error_popup_shown {
                            lf.error_popup_shown = true;
                            show_error_popup = true;
                        }
                    }
                }

                if should_mark_disconnected {
                    eprintln!("[LASTFM] watchdog error: {e}");
                    if show_error_popup {
                        (state.popup_notifier)(build_lastfm_runtime_error_popup(&e));
                    }
                } else {
                    eprintln!("[LASTFM] watchdog key refresh failed (suppressed): {e}");
                }
            }
        }
    }
}

pub async fn handle_health(State(state): State<ServerState>) -> impl IntoResponse {
    let ext = state.extension.lock().await;

    (
        StatusCode::OK,
        Json(HealthResponse {
            ok: true,
            app: "yamusic_lastfm_popup",
            version: env!("CARGO_PKG_VERSION"),
            extension_connected: ext.connected,
            extension_last_seen_at: ext.last_seen_at,
        }),
    )
}

pub async fn handle_extension_ping(
    State(state): State<ServerState>,
    Json(payload): Json<ExtensionPingRequest>,
) -> impl IntoResponse {
    let client_name = payload.client_name.as_deref().unwrap_or("");
    let client_version = payload.client_version.as_deref().unwrap_or("");

    println!(
        "[EXTENSION_PING] client='{}' version='{}' schema={:?} sent_at={:?} yandex_tab_open={:?} metadata_active={:?} reload_likely_needed={:?}",
        client_name,
        client_version,
        payload.schema_version,
        payload.sent_at,
        payload.yandex_tab_open,
        payload.metadata_active,
        payload.reload_likely_needed
    );

    mark_extension_seen(&state, "ping").await;

    let mut should_show_reload_popup = false;

    {
        let mut ext = state.extension.lock().await;

        ext.yandex_tab_open = payload.yandex_tab_open.unwrap_or(false);
        ext.metadata_active = payload.metadata_active.unwrap_or(false);
        ext.reload_likely_needed = payload.reload_likely_needed.unwrap_or(false);

        if ext.reload_likely_needed {
            if !ext.reload_popup_shown {
                ext.reload_popup_shown = true;
                should_show_reload_popup = true;
            }
        } else {
            ext.reload_popup_shown = false;
        }
    }

    if should_show_reload_popup {
        println!("[EXTENSION] reload popup requested");
        (state.popup_notifier)(build_reload_needed_popup());
    }

    let ext = state.extension.lock().await;

    (
        StatusCode::OK,
        Json(ExtensionPingResponse {
            ok: true,
            app: "yamusic_lastfm_popup",
            version: env!("CARGO_PKG_VERSION"),
            extension_connected: ext.connected,
            extension_last_seen_at: ext.last_seen_at,
        }),
    )
}

pub async fn handle_companion_import_lastfm(
    State(state): State<ServerState>,
    Json(payload): Json<CompanionImportLastfmRequest>,
) -> impl IntoResponse {
    let source = payload
        .source
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let mut normalized = LastfmConfig {
        api_key: payload.api_key.trim().to_string(),
        api_secret: payload.api_secret.trim().to_string(),
        username: payload.username.trim().to_string(),
        password: String::new(),
        session_key: payload.session_key.trim().to_string(),
        synced_from_extension: true,
        auth_token: String::new(),
        auth_token_requested_at: 0,
    };

    let mut config = load_app_config()
        .unwrap_or(None)
        .unwrap_or_else(|| AppConfig::default());

    if normalized.username.is_empty() {
        normalized.username = config.lastfm.username.trim().to_string();
    }

    if normalized.api_key.is_empty()
        || normalized.api_secret.is_empty()
        || normalized.session_key.is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(CompanionImportLastfmResponse {
                ok: false,
                imported: false,
                source,
                username: normalized.username.clone(),
                has_session_key: false,
            }),
        );
    }

    config.lastfm.api_key = normalized.api_key.clone();
    config.lastfm.api_secret = normalized.api_secret.clone();
    config.lastfm.username = normalized.username.clone();
    config.lastfm.session_key = normalized.session_key.clone();
    config.lastfm.synced_from_extension = true;

    if let Err(err) = save_app_config(&config) {
        eprintln!("[COMPANION_IMPORT] save_app_config error: {}", err);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CompanionImportLastfmResponse {
                ok: false,
                imported: false,
                source,
                username: normalized.username.clone(),
                has_session_key: true,
            }),
        );
    }

    {
        let mut cfg = state.lastfm_config.lock().await;
        cfg.api_key = normalized.api_key.clone();
        cfg.api_secret = normalized.api_secret.clone();
        cfg.username = normalized.username.clone();
        cfg.session_key = normalized.session_key.clone();
        cfg.synced_from_extension = true;
    }

    {
        let mut sk = state.session_key.lock().await;
        *sk = Some(normalized.session_key.clone());
    }

    {
        let mut lf = state.lastfm_runtime.lock().await;
        lf.connected = true;
        lf.last_error = None;
        lf.error_popup_shown = false;
        lf.watchdog_fail_count = 0;
        lf.last_success_at = Some(unix_ts());
    }

    println!(
        "[COMPANION_IMPORT] source='{}' username='{}' session_key_imported=true",
        source, normalized.username
    );

    (
        StatusCode::OK,
        Json(CompanionImportLastfmResponse {
            ok: true,
            imported: true,
            source,
            username: normalized.username,
            has_session_key: true,
        }),
    )
}

pub async fn handle_companion_export_lastfm(State(state): State<ServerState>) -> impl IntoResponse {
    let cfg = {
        let cfg = state.lastfm_config.lock().await;
        cfg.clone()
    };

    if !cfg.has_companion_auth() && !cfg.has_full_credentials() {
        return (
            StatusCode::NOT_FOUND,
            Json(CompanionExportLastfmResponse {
                ok: false,
                source: "winapp".to_string(),
                api_key: String::new(),
                api_secret: String::new(),
                username: String::new(),
                session_key: String::new(),
                synced_from_extension: cfg.synced_from_extension,
            }),
        );
    }

    (
        StatusCode::OK,
        Json(CompanionExportLastfmResponse {
            ok: true,
            source: "winapp".to_string(),
            api_key: cfg.api_key,
            api_secret: cfg.api_secret,
            username: cfg.username,
            session_key: cfg.session_key,
            synced_from_extension: cfg.synced_from_extension,
        }),
    )
}

pub async fn run_server(
    lastfm_config: LastfmConfig,
    popup_notifier: PopupNotifier,
    extension_status_notifier: ExtensionStatusNotifier,
) -> Result<(), String> {
    let normalized_lastfm_config = lastfm_config.normalized();

    let client = Client::builder()
        .build()
        .map_err(|e| format!("reqwest client error: {e}"))?;

    let initial_session_key = if normalized_lastfm_config.session_key.trim().is_empty() {
        None
    } else {
        Some(normalized_lastfm_config.session_key.trim().to_string())
    };

    let mut initial_lastfm_runtime = LastfmRuntimeState::default();
    if initial_session_key.is_some() {
        initial_lastfm_runtime.connected = true;
        initial_lastfm_runtime.last_error = None;
        initial_lastfm_runtime.last_success_at = Some(unix_ts());
    }

    let state = ServerState {
        client,
        lastfm_config: Arc::new(Mutex::new(normalized_lastfm_config)),
        session_key: Arc::new(Mutex::new(initial_session_key)),
        playback: Arc::new(Mutex::new(PlaybackState::default())),
        extension: Arc::new(Mutex::new(ExtensionRuntimeState::default())),
        lastfm_runtime: Arc::new(Mutex::new(initial_lastfm_runtime)),
        popup_notifier,
        extension_status_notifier,
    };

    let _ = SERVER_STATE_HANDLE.set(state.clone());

    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/extension/ping", post(handle_extension_ping))
        .route(
            "/companion/import-lastfm",
            post(handle_companion_import_lastfm),
        )
        .route(
            "/companion/export-lastfm",
            get(handle_companion_export_lastfm),
        )
        .route("/track", post(handle_track))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(SERVER_BIND_ADDR)
        .await
        .map_err(|e| format!("bind error: {e}"))?;

    println!("Listening on http://{SERVER_BIND_ADDR}");

    tokio::spawn(scrobble_worker(state.clone()));
    tokio::spawn(extension_watchdog_worker(state.clone()));
    tokio::spawn(initial_extension_popup_worker(state.clone()));
    tokio::spawn(lastfm_watchdog_worker(state.clone()));

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("serve error: {e}"))
}

pub async fn handle_track(
    State(state): State<ServerState>,
    Json(payload): Json<IncomingTrack>,
) -> impl IntoResponse {
    let artist = payload.artist.trim().to_string();
    let track = payload.track.trim().to_string();
    let album = payload.album.as_deref().unwrap_or("").trim().to_string();
    let cover_url = payload
        .cover_url
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();

    let duration = payload.duration;
    let event_type = payload.event_type.as_deref().unwrap_or("");
    let event_id = payload.event_id.as_deref().unwrap_or("");
    let page_url = payload.page_url.as_deref().unwrap_or("");
    let client_name = payload.client_name.as_deref().unwrap_or("");

    if artist.is_empty() || track.is_empty() {
        return (StatusCode::BAD_REQUEST, "missing artist/track").into_response();
    }

    mark_extension_seen(&state, "track").await;

    let current = format!("{artist} - {track}");
    let now = unix_ts();

    let client_started_at_present = payload.started_at.is_some();
    let started_at = sanitize_track_started_at(payload.started_at, now);
    let scrobble_due_at = payload
        .scrobble_due_at
        .unwrap_or(started_at + SCROBBLE_AFTER_SECS);

    let historical_due =
        client_started_at_present && now.saturating_sub(started_at) >= SCROBBLE_AFTER_SECS;

    println!(
        "[TRACK_IN] client='{}' event_type='{}' event_id='{}' artist='{}' track='{}' album='{}' duration={:?} started_at={} scrobble_due_at={} historical_due={} page_url='{}'",
        client_name,
        event_type,
        event_id,
        artist,
        track,
        album,
        duration,
        started_at,
        scrobble_due_at,
        historical_due,
        page_url
    );

    if historical_due {
        let pending_item = PendingScrobble {
            artist: artist.clone(),
            track: track.clone(),
            album: if album.is_empty() {
                None
            } else {
                Some(album.clone())
            },
            timestamp: started_at,
            duration,
            queued_at: now,
            retry_count: 0,
        };

        if let Err(enq_err) = enqueue_pending_scrobble(pending_item) {
            eprintln!(
                "[PENDING_SCROBBLES] historical enqueue error => artist='{}' track='{}' err={}",
                artist, track, enq_err
            );

            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("pending scrobble enqueue error: {enq_err}"),
            )
                .into_response();
        }

        println!(
            "[PENDING_SCROBBLES] historical queued => artist='{}' track='{}' timestamp={}",
            artist, track, started_at
        );

        let flush_state = state.clone();
        tokio::spawn(async move {
            match flush_pending_scrobbles(&flush_state).await {
                Ok(n) if n > 0 => {
                    println!(
                        "[PENDING_SCROBBLES] historical flush started, flushed={}",
                        n
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[PENDING_SCROBBLES] historical flush error: {}", e);
                }
            }
        });

        return (
            StatusCode::OK,
            Json(TrackAcceptedResponse {
                ok: true,
                accepted: true,
                new_track: false,
            }),
        )
            .into_response();
    }

    let is_new_track = {
        let mut pb = state.playback.lock().await;

        println!(
            "[SCROBBLE_DEBUG] incoming current='{}' last_track={:?} started_at={} scrobbled={}",
            current, pb.last_track, pb.started_at, pb.scrobbled
        );

        if pb.last_track.as_deref() != Some(current.as_str()) {
            println!("[SCROBBLE_DEBUG] NEW TRACK => reset timer");

            pb.last_track = Some(current.clone());
            pb.last_artist = Some(artist.clone());
            pb.last_title = Some(track.clone());
            pb.last_album = if album.is_empty() {
                None
            } else {
                Some(album.clone())
            };
            pb.started_at = started_at;
            pb.scrobbled = false;
            pb.last_scrobble_error = None;
            pb.last_scrobble_error_at = None;

            true
        } else {
            let elapsed = now - pb.started_at;

            println!(
                "[SCROBBLE_DEBUG] SAME TRACK => elapsed={} sec, scrobbled={}",
                elapsed, pb.scrobbled
            );

            false
        }
    };

    if is_new_track {
        println!("Now playing: {}", current);

        let (cover_path, dominant_rgb) = if !cover_url.is_empty() {
            match download_cover_art(&state.client, &cover_url).await {
                Some(bytes) => {
                    let dominant_rgb = dominant_rgb_from_bytes(&bytes);
                    let cover_path = save_cover_temp_file(&bytes).ok();

                    (cover_path, dominant_rgb)
                }
                None => (None, None),
            }
        } else {
            (None, None)
        };

        (state.popup_notifier)(build_track_popup(
            &artist,
            &track,
            if cover_url.is_empty() {
                None
            } else {
                Some(cover_url)
            },
            cover_path,
            dominant_rgb,
        ));
    }

    (
        StatusCode::OK,
        Json(TrackAcceptedResponse {
            ok: true,
            accepted: true,
            new_track: is_new_track,
        }),
    )
        .into_response()
}

pub fn build_track_popup(
    artist: &str,
    track: &str,
    cover_url: Option<String>,
    cover_path: Option<String>,
    dominant_rgb: Option<[u8; 3]>,
) -> PopupPayload {
    PopupPayload {
        kind: PopupKind::Track,
        title: artist.to_string(),
        line1: track.to_string(),
        line2: String::new(),
        footer: APP_FOOTER_TEXT.to_string(),
        cover_url,
        cover_path,
        dominant_rgb,
    }
}

fn status_icon_path(kind: &PopupKind) -> Option<String> {
    let rel = match kind {
        PopupKind::Track => return None,
        PopupKind::StartupOk => "assets/status_ok.png",
        PopupKind::StartupError => "assets/status_error.png",
        PopupKind::ExtensionMissing => "assets/status_extension_missing.png",
        PopupKind::ReloadNeeded => "assets/status_reload.png",
    };

    let mut path = std::env::current_exe().ok()?;
    path.pop();
    path.push(rel);
    Some(path.to_string_lossy().to_string())
}

fn build_status_popup(kind: PopupKind, title: &str, line1: &str, line2: &str) -> PopupPayload {
    let cover_path = status_icon_path(&kind);

    PopupPayload {
        kind,
        title: title.to_string(),
        line1: line1.to_string(),
        line2: line2.to_string(),
        footer: APP_FOOTER_TEXT.to_string(),
        cover_url: None,
        cover_path,
        dominant_rgb: None,
    }
}

pub fn build_startup_ok_popup() -> PopupPayload {
    build_status_popup(
        PopupKind::StartupOk,
        "Last.fm",
        "Connection established",
        "",
    )
}

pub fn build_reload_needed_popup() -> PopupPayload {
    build_status_popup(
        PopupKind::ReloadNeeded,
        "Yandex Music",
        "Reload tab to start scrobbling",
        "",
    )
}

pub fn build_extension_missing_popup() -> PopupPayload {
    build_status_popup(
        PopupKind::ExtensionMissing,
        "Chrome extension not connected",
        "Start extension to continue scrobbling",
        "",
    )
}

pub fn build_lastfm_runtime_error_popup(error: &str) -> PopupPayload {
    build_status_popup(
        PopupKind::StartupError,
        "Last.fm",
        "Connection not available",
        error,
    )
}

pub async fn download_cover_art(client: &Client, cover_url: &str) -> Option<Vec<u8>> {
    match tokio::time::timeout(Duration::from_millis(COVER_DOWNLOAD_TIMEOUT_MS), async {
        let resp = client.get(cover_url).send().await.ok()?;
        let bytes = resp.bytes().await.ok()?;
        Some(bytes.to_vec())
    })
    .await
    {
        Ok(bytes_opt) => bytes_opt,
        Err(_) => None,
    }
}

pub fn save_cover_temp_file(bytes: &[u8]) -> Result<String, String> {
    let mut path = std::env::temp_dir();
    path.push(temp_cover_filename());

    fs::write(&path, bytes).map_err(|e| format!("cover temp write error: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

pub fn temp_cover_filename() -> String {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    format!("yamusic_lastfm_cover_{ts_ms}.img")
}

pub fn dominant_rgb_from_bytes(bytes: &[u8]) -> Option<[u8; 3]> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgb = img.to_rgb8();

    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;

    let step_x = (rgb.width().max(1) / DOMINANT_SAMPLE_GRID).max(1);
    let step_y = (rgb.height().max(1) / DOMINANT_SAMPLE_GRID).max(1);

    for y in (0..rgb.height()).step_by(step_y as usize) {
        for x in (0..rgb.width()).step_by(step_x as usize) {
            let p = rgb.get_pixel(x, y);
            let [r, g, b] = p.0;

            let maxc = r.max(g).max(b);
            let minc = r.min(g).min(b);
            let sat = maxc.saturating_sub(minc);

            if maxc < DOMINANT_MIN_BRIGHTNESS || sat < DOMINANT_MIN_SATURATION {
                continue;
            }

            r_sum += r as u64;
            g_sum += g as u64;
            b_sum += b as u64;
            count += 1;
        }
    }

    if count == 0 {
        return None;
    }

    Some([
        (r_sum / count) as u8,
        (g_sum / count) as u8,
        (b_sum / count) as u8,
    ])
}

pub fn unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn sanitize_track_started_at(value: Option<i64>, now: i64) -> i64 {
    let Some(started_at) = value else {
        return now;
    };

    if started_at <= 0 {
        return now;
    }

    // Do not accept future timestamps from a buggy/stale client.
    if started_at > now + 5 {
        return now;
    }

    // Last.fm accepts historical scrobbles only within a bounded past window.
    // Keep this conservative and avoid treating very old queued metadata as active playback.
    let oldest_allowed = now - 14 * 24 * 60 * 60;
    if started_at < oldest_allowed {
        return now;
    }

    started_at
}
