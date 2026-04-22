use crate::app_config::LastfmConfig;
use crate::config::{API_URL, SESSION_KEY_RETRY_BASE_DELAY_SECS, SESSION_KEY_RETRY_COUNT};
use crate::models::PendingScrobble;

use reqwest::Client;
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::time::{sleep, Duration};

fn is_permanent_session_key_error(err: &str) -> bool {
    let e = err.to_ascii_lowercase();

    e.contains("http 401")
        || e.contains("http 403")
        || e.contains("invalid api key")
        || e.contains("invalid method signature")
        || e.contains("authentication failed")
        || e.contains("invalid username")
        || e.contains("invalid password")
        || e.contains("last.fm error 4")
        || e.contains("last.fm error 9")
        || e.contains("last.fm error 10")
        || e.contains("last.fm error 26")
}

pub async fn get_session_key_with_retry(
    client: &Client,
    lastfm: &LastfmConfig,
) -> Result<String, String> {
    let mut last_err = String::new();

    for attempt in 1..=SESSION_KEY_RETRY_COUNT {
        match get_session_key(client, lastfm).await {
            Ok(sk) => return Ok(sk),
            Err(e) => {
                eprintln!(
                    "get_session_key attempt {attempt}/{} failed: {e}",
                    SESSION_KEY_RETRY_COUNT
                );

                if is_permanent_session_key_error(&e) {
                    return Err(format!("Last.fm authentication failed: {e}"));
                }

                last_err = e;

                if attempt < SESSION_KEY_RETRY_COUNT {
                    let delay_secs = SESSION_KEY_RETRY_BASE_DELAY_SECS * attempt as u64;
                    sleep(Duration::from_secs(delay_secs)).await;
                }
            }
        }
    }

    Err(format!(
        "Could not get Last.fm session key after {} attempts: {last_err}",
        SESSION_KEY_RETRY_COUNT
    ))
}

pub async fn get_session_key(client: &Client, lastfm: &LastfmConfig) -> Result<String, String> {
    let lastfm = lastfm.normalized();

    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "auth.getMobileSession".to_string());
    params.insert("username".to_string(), lastfm.username.clone());
    params.insert("password".to_string(), lastfm.password.clone());
    params.insert("api_key".to_string(), lastfm.api_key.clone());

    let result = post_lastfm(client, &lastfm, params).await?;

    result
        .get("session")
        .and_then(|s| s.get("key"))
        .and_then(|k| k.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Не удалось получить session key: {result}"))
}

pub async fn scrobble(
    client: &Client,
    lastfm: &LastfmConfig,
    sk: &str,
    artist: &str,
    track: &str,
    timestamp: i64,
) -> Result<Value, String> {
    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "track.scrobble".to_string());
    params.insert("artist".to_string(), artist.to_string());
    params.insert("track".to_string(), track.to_string());
    params.insert("timestamp".to_string(), timestamp.to_string());
    params.insert("sk".to_string(), sk.to_string());
    params.insert("api_key".to_string(), lastfm.api_key.trim().to_string());

    post_lastfm(client, lastfm, params).await
}

pub async fn scrobble_batch(
    client: &Client,
    lastfm: &LastfmConfig,
    sk: &str,
    items: &[PendingScrobble],
) -> Result<Value, String> {
    if items.is_empty() {
        return Err("scrobble_batch called with empty items".to_string());
    }

    if items.len() > 50 {
        return Err("scrobble_batch max size is 50".to_string());
    }

    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "track.scrobble".to_string());
    params.insert("sk".to_string(), sk.to_string());
    params.insert("api_key".to_string(), lastfm.api_key.trim().to_string());

    for (i, item) in items.iter().enumerate() {
        params.insert(format!("artist[{i}]"), item.artist.clone());
        params.insert(format!("track[{i}]"), item.track.clone());
        params.insert(format!("timestamp[{i}]"), item.timestamp.to_string());

        if let Some(album) = item.album.as_ref() {
            if !album.trim().is_empty() {
                params.insert(format!("album[{i}]"), album.clone());
            }
        }
    }

    post_lastfm(client, lastfm, params).await
}

pub async fn post_lastfm(
    client: &Client,
    lastfm: &LastfmConfig,
    mut params: BTreeMap<String, String>,
) -> Result<Value, String> {
    let api_sig = build_api_sig(&params, lastfm.api_secret.trim());
    params.insert("api_sig".to_string(), api_sig);
    params.insert("format".to_string(), "json".to_string());

    let resp = client
        .post(API_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("HTTP send error: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("HTTP body read error: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }

    serde_json::from_str::<Value>(&body).map_err(|e| format!("JSON parse error: {e}; body={body}"))
}

pub fn build_api_sig(params: &BTreeMap<String, String>, api_secret: &str) -> String {
    let mut raw = String::new();

    for (k, v) in params {
        if k == "format" || k == "callback" || k == "api_sig" {
            continue;
        }

        raw.push_str(k);
        raw.push_str(v);
    }

    raw.push_str(api_secret);

    format!("{:x}", md5::compute(raw))
}
