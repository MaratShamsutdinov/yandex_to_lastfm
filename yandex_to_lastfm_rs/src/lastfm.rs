use crate::app_config::LastfmConfig;
use crate::config::API_URL;
use crate::models::PendingScrobble;

use reqwest::Client;
use serde_json::Value;
use std::collections::BTreeMap;

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

pub async fn get_auth_token(client: &Client, lastfm: &LastfmConfig) -> Result<String, String> {
    let lastfm = lastfm.normalized();

    if lastfm.api_key.is_empty() || lastfm.api_secret.is_empty() {
        return Err("Last.fm API Key / API Secret missing".to_string());
    }

    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "auth.getToken".to_string());
    params.insert("api_key".to_string(), lastfm.api_key.clone());

    let result = post_lastfm(client, &lastfm, params).await?;

    result
        .get("token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Не удалось получить auth token: {result}"))
}

pub async fn get_session_key_from_token(
    client: &Client,
    lastfm: &LastfmConfig,
    token: &str,
) -> Result<String, String> {
    let lastfm = lastfm.normalized();
    let token = token.trim();

    if token.is_empty() {
        return Err("Last.fm auth token is empty".to_string());
    }

    let mut params = BTreeMap::new();
    params.insert("method".to_string(), "auth.getSession".to_string());
    params.insert("token".to_string(), token.to_string());
    params.insert("api_key".to_string(), lastfm.api_key.clone());

    let result = post_lastfm(client, &lastfm, params).await?;

    result
        .get("session")
        .and_then(|s| s.get("key"))
        .and_then(|k| k.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Не удалось получить session key из token: {result}"))
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

    let parsed = serde_json::from_str::<Value>(&body)
        .map_err(|e| format!("JSON parse error: {e}; body={body}"))?;

    if let Some(error_value) = parsed.get("error") {
        let code = error_value
            .as_i64()
            .map(|v| v.to_string())
            .or_else(|| error_value.as_str().map(|v| v.to_string()))
            .unwrap_or_else(|| error_value.to_string());

        let message = parsed
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Last.fm API error");

        return Err(format!("Last.fm error {code}: {message}; body={parsed}"));
    }

    Ok(parsed)
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
