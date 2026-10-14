//! X chat source for the unified studio chat.
//!
//! Harry wants his X mentions/replies in the same dashboard as his other
//! chat — not a separate tool. This module polls the X API v2 mentions
//! timeline for his handle and returns simplified posts.
//!
//! COST REALITY (2026): X retired the free read tier. Post reads are
//! pay-per-use (~$0.005/post). A 4-hour stream polling every 2 minutes
//! costs a few dollars. The app NEVER hides this — the UI shows the
//! cost basis and the token is Harry's own (Bearer, stored locally,
//! never in the repo or sent anywhere except api.x.com).
//!
//! Auth: Bearer token (app-only) for mentions reads. Harry pastes his own
//! token from developer.x.com into the Chat tab; it's kept in the Tauri
//! app-data config, never logged.

use serde::{Deserialize, Serialize};
use tauri::command;

/// One X post, simplified for the chat panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XPost {
    pub id: String,
    pub author: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
struct XApiResponse {
    data: Option<Vec<XApiPost>>,
    meta: Option<XApiMeta>,
}

#[derive(Debug, Deserialize)]
struct XApiPost {
    id: String,
    text: String,
    created_at: Option<String>,
    author_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XApiMeta {
    newest_id: Option<String>,
    result_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct XUserResponse {
    data: Option<XApiUser>,
}

#[derive(Debug, Deserialize)]
struct XApiUser {
    id: String,
    username: String,
}

/// Resolve a handle to a user ID (needed for the mentions endpoint).
#[command]
pub fn x_resolve_user(bearer: String, username: String) -> Result<String, String> {
    let username = username.trim_start_matches('@');
    let url = format!("https://api.x.com/2/users/by/username/{}", username);
    let body = http_get(&url, &bearer)?;
    let parsed: XUserResponse =
        serde_json::from_str(&body).map_err(|e| format!("bad user response: {e}"))?;
    parsed
        .data
        .map(|u| u.id)
        .ok_or_else(|| format!("no X user found for @{username}"))
}

/// Poll mentions for a user ID. `since_id` skips already-seen posts.
/// Returns (posts, newest_id) — the UI stores newest_id for the next poll.
#[command]
pub fn x_poll_mentions(
    bearer: String,
    user_id: String,
    since_id: Option<String>,
) -> Result<(Vec<XPost>, Option<String>), String> {
    let mut url = format!(
        "https://api.x.com/2/users/{}/mentions?max_results=20&tweet.fields=created_at,author_id&expansions=author_id&user.fields=username",
        user_id
    );
    if let Some(sid) = since_id {
        if !sid.is_empty() {
            url.push_str(&format!("&since_id={}", sid));
        }
    }
    let body = http_get(&url, &bearer)?;
    let parsed: XApiResponse =
        serde_json::from_str(&body).map_err(|e| format!("bad mentions response: {e}"))?;

    // Build author lookup from includes (best effort — fall back to ID).
    let mut authors: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(users) = v.get("includes").and_then(|i| i.get("users")).and_then(|u| u.as_array()) {
            for u in users {
                if let (Some(id), Some(name)) = (
                    u.get("id").and_then(|x| x.as_str()),
                    u.get("username").and_then(|x| x.as_str()),
                ) {
                    authors.insert(id.to_string(), format!("@{name}"));
                }
            }
        }
    }

    let posts: Vec<XPost> = parsed
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let author = p
                .author_id
                .as_deref()
                .and_then(|a| authors.get(a))
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            XPost {
                id: p.id,
                author,
                text: p.text,
                created_at: p.created_at.unwrap_or_default(),
            }
        })
        .collect();

    let newest = parsed.meta.and_then(|m| m.newest_id);
    Ok((posts, newest))
}

/// Minimal HTTPS GET via PowerShell (no extra crates — the app already
/// shells to PowerShell for WMI probes). Returns the response body.
/// Fails loudly on 403 (not enrolled / no paid access) so the UI can
/// explain the billing problem instead of retrying blindly.
#[cfg(target_os = "windows")]
fn http_get(url: &str, bearer: &str) -> Result<String, String> {
    use std::process::Command;
    // Escape single quotes for PowerShell.
    let safe_url = url.replace('\'', "''");
    let safe_bearer = bearer.replace('\'', "''");
    let ps = format!(
        "$r = Invoke-WebRequest -Uri '{}' -Headers @{{ Authorization = 'Bearer {}' }} -UseBasicParsing -ErrorAction Stop; $r.Content",
        safe_url, safe_bearer
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
        .output()
        .map_err(|e| format!("request failed: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("403") {
            return Err("X API returned 403 — the app isn't enrolled for paid reads. Check developer.x.com billing (pay-per-use credits).".to_string());
        }
        return Err(format!("X API request failed: {}", err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(not(target_os = "windows"))]
fn http_get(_url: &str, _bearer: &str) -> Result<String, String> {
    Err("X chat polling is implemented for Windows only.".to_string())
}
