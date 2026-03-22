// ============================================================
// Supabase Sync — Phase 4
// ============================================================
// Commands for auth + bidirectional cloud sync with Supabase.
// JWT stored in OS keychain; URL/anon_key in app settings.
// ============================================================

use crate::storage::records::{detect_central_home_path, load_records, normalized_home};
use crate::storage::settings_io::{load_settings, save_settings};
use crate::types::OPENAI_SERVICE;
use keyring::Entry;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

const SUPABASE_JWT_KEY: &str = "supabase_jwt";
const SUPABASE_REFRESH_KEY: &str = "supabase_refresh_token";
const SUPABASE_USER_ID_KEY: &str = "supabase_user_id";
const SUPABASE_EMAIL_KEY: &str = "supabase_email";

// ── Keychain helpers ──────────────────────────────────────────────────────────

fn kc_set(username: &str, value: &str) -> Result<(), String> {
    Entry::new(OPENAI_SERVICE, username)
        .map_err(|e| e.to_string())?
        .set_password(value)
        .map_err(|e| e.to_string())
}

fn kc_get(username: &str) -> Option<String> {
    Entry::new(OPENAI_SERVICE, username)
        .ok()?
        .get_password()
        .ok()
        .filter(|v| !v.trim().is_empty())
}

fn kc_delete(username: &str) {
    if let Ok(entry) = Entry::new(OPENAI_SERVICE, username) {
        let _ = entry.delete_password();
    }
}

// ── Auth ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct SupabaseTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    user: SupabaseUser,
}

#[derive(Debug, Serialize, Deserialize)]
struct SupabaseUser {
    id: String,
    email: Option<String>,
}

#[tauri::command]
pub(crate) fn supabase_sign_in(email: String, password: String) -> Result<Value, String> {
    let settings = load_settings();
    let url = settings.integrations.supabase.url.trim().to_string();
    let anon_key = settings.integrations.supabase.anon_key.trim().to_string();

    if url.is_empty() || anon_key.is_empty() {
        return Err("請先在設定中填入 Supabase URL 和 Anon Key".to_string());
    }

    let client = Client::new();
    let res = client
        .post(format!("{}/auth/v1/token?grant_type=password", url))
        .header("apikey", &anon_key)
        .header("Content-Type", "application/json")
        .json(&json!({ "email": email, "password": password }))
        .send()
        .map_err(|e| format!("網路錯誤: {}", e))?;

    if res.status() == 400 {
        return Err("帳號或密碼錯誤".to_string());
    }
    if !res.status().is_success() {
        return Err(format!("登入失敗（{}）", res.status()));
    }

    let body: SupabaseTokenResponse = res.json().map_err(|e| e.to_string())?;
    let user_email = body.user.email.clone().unwrap_or_default();

    kc_set(SUPABASE_JWT_KEY, &body.access_token)?;
    if let Some(rt) = &body.refresh_token {
        let _ = kc_set(SUPABASE_REFRESH_KEY, rt);
    }
    kc_set(SUPABASE_USER_ID_KEY, &body.user.id)?;
    kc_set(SUPABASE_EMAIL_KEY, &user_email)?;

    Ok(json!({
        "signed_in": true,
        "user_id": body.user.id,
        "email": user_email,
    }))
}

#[tauri::command]
pub(crate) fn supabase_sign_out() -> Result<(), String> {
    kc_delete(SUPABASE_JWT_KEY);
    kc_delete(SUPABASE_REFRESH_KEY);
    kc_delete(SUPABASE_USER_ID_KEY);
    kc_delete(SUPABASE_EMAIL_KEY);
    Ok(())
}

// ── JWT refresh ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

/// Try to refresh the Supabase JWT using the stored refresh_token.
/// Updates keychain on success. Returns new access_token or an error
/// message prompting the user to sign in again.
fn try_refresh_token(client: &Client, url: &str, anon_key: &str) -> Result<String, String> {
    let refresh_token = kc_get(SUPABASE_REFRESH_KEY)
        .ok_or("JWT 已過期，請重新登入（無 refresh token）")?;

    let res = client
        .post(format!("{}/auth/v1/token?grant_type=refresh_token", url))
        .header("apikey", anon_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .map_err(|e| format!("網路錯誤（refresh）: {}", e))?;

    if res.status() == 400 || res.status() == 401 {
        // Refresh token revoked or expired — clear credentials
        kc_delete(SUPABASE_JWT_KEY);
        kc_delete(SUPABASE_REFRESH_KEY);
        return Err("登入已失效，請重新登入".to_string());
    }
    if !res.status().is_success() {
        return Err(format!("Token refresh 失敗（{}）", res.status()));
    }

    let body: RefreshTokenResponse = res.json().map_err(|e| e.to_string())?;
    kc_set(SUPABASE_JWT_KEY, &body.access_token)?;
    if let Some(rt) = &body.refresh_token {
        let _ = kc_set(SUPABASE_REFRESH_KEY, rt);
    }
    Ok(body.access_token)
}

#[tauri::command]
pub(crate) fn supabase_auth_status() -> Value {
    let jwt = kc_get(SUPABASE_JWT_KEY);
    json!({
        "signed_in": jwt.is_some(),
        "email": kc_get(SUPABASE_EMAIL_KEY).unwrap_or_default(),
        "user_id": kc_get(SUPABASE_USER_ID_KEY).unwrap_or_default(),
    })
}

// ── Sync ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncStats {
    pub pushed: usize,
    pub pulled: usize,
    pub failed: usize,
}

#[tauri::command]
pub(crate) fn supabase_full_sync() -> Result<SyncStats, String> {
    let settings = load_settings();
    let supabase = &settings.integrations.supabase;
    let url = supabase.url.trim().to_string();
    let anon_key = supabase.anon_key.trim().to_string();
    let last_sync = supabase.last_sync_at.clone();

    let client = Client::new();

    // Proactively refresh JWT to avoid mid-sync 401s (Supabase tokens expire in 1h)
    let jwt = match kc_get(SUPABASE_JWT_KEY) {
        Some(existing) => match try_refresh_token(&client, &url, &anon_key) {
            Ok(new_token) => new_token,
            // Fatal: credentials revoked — surface error, do not attempt sync
            Err(e) if e.contains("失效") || e.contains("重新登入") => return Err(e),
            // Non-fatal: no refresh token stored or network glitch — use existing JWT
            Err(_) => existing,
        },
        None => return Err("未登入 Supabase，請先在設定中登入".to_string()),
    };
    let user_id = kc_get(SUPABASE_USER_ID_KEY).ok_or("找不到用戶 ID")?;

    if url.is_empty() {
        return Err("請先在設定中填入 Supabase URL".to_string());
    }

    // Resolve central_home from active profile (or first profile)
    let central_home_str = settings
        .profiles
        .iter()
        .find(|p| Some(&p.id) == settings.active_profile_id.as_ref())
        .or_else(|| settings.profiles.first())
        .map(|p| p.central_home.clone())
        .filter(|s| !s.is_empty())
        .ok_or("找不到 Central Home，請先在設定中建立設定檔")?;
    let central_home_pb =
        normalized_home(&central_home_str).map_err(|e| e.to_string())?;
    let central_home = central_home_pb.to_string_lossy().to_string();

    let mut pushed = 0usize;
    let mut failed = 0usize;

    // 1. Push all local records to Supabase (upsert idempotent)
    let records = load_records(Path::new(&central_home)).map_err(|e| e.to_string())?;
    for record in &records {
        match push_record(&client, &url, &anon_key, &jwt, &user_id, record) {
            Ok(_) => pushed += 1,
            Err(e) => {
                eprintln!("[Supabase] push failed for {:?}: {}", record.json_path, e);
                failed += 1;
            }
        }
    }

    // 2. Pull remote changes since last sync
    let pulled = pull_records(&client, &url, &anon_key, &jwt, &user_id, &last_sync, &central_home)
        .unwrap_or_else(|e| {
            eprintln!("[Supabase] pull failed: {}", e);
            0
        });

    // 3. Update last_sync_at in settings
    let mut updated_settings = settings;
    updated_settings.integrations.supabase.last_sync_at =
        chrono::Utc::now().to_rfc3339();
    let _ = save_settings(&updated_settings);

    Ok(SyncStats { pushed, pulled, failed })
}

/// Derive a stable UUID-shaped string from a local record path.
/// Uses two DefaultHasher passes to produce 128 bits of hash and
/// formats them as a lowercase UUID (no crypto dependency needed).
fn path_to_uuid(local_id: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    local_id.hash(&mut h);
    let hi = h.finish();
    let mut h2 = DefaultHasher::new();
    hi.hash(&mut h2);
    "desktop_record".hash(&mut h2);
    let lo = h2.finish();
    let b = [
        (hi >> 56) as u8, (hi >> 48) as u8, (hi >> 40) as u8, (hi >> 32) as u8,
        (hi >> 24) as u8, (hi >> 16) as u8, (hi >> 8) as u8, hi as u8,
        (lo >> 56) as u8, (lo >> 48) as u8, (lo >> 40) as u8, (lo >> 32) as u8,
        (lo >> 24) as u8, (lo >> 16) as u8, (lo >> 8) as u8, lo as u8,
    ];
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn push_record(
    client: &Client,
    url: &str,
    anon_key: &str,
    jwt: &str,
    user_id: &str,
    record: &crate::types::Record,
) -> Result<(), String> {
    let tags: Vec<String> = record.tags.clone();
    // Use json_path (or title) as a stable local identifier for conflict resolution
    let local_id = record
        .json_path
        .as_deref()
        .unwrap_or(&record.title);
    let record_uuid = path_to_uuid(local_id);
    let payload = json!({
        "p_id": record_uuid,
        "p_user_id": user_id,
        "p_local_id": local_id,
        "p_device_id": "desktop",
        "p_record_type": record.record_type,
        "p_title": record.title,
        "p_source_text": record.source_text,
        "p_final_body": record.final_body,
        "p_tags": tags,
        "p_source_url": Value::Null,
        "p_source_platform": "desktop",
        "p_key_insight": Value::Null,
        "p_date": record.date.as_deref().or_else(|| record.created_at.get(..10)),
        "p_is_deleted": false,
        "p_updated_at": record.created_at,
    });

    let res = client
        .post(format!("{}/rest/v1/rpc/upsert_record", url))
        .header("apikey", anon_key)
        .header("Authorization", format!("Bearer {}", jwt))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }
    Ok(())
}

fn pull_records(
    client: &Client,
    url: &str,
    anon_key: &str,
    jwt: &str,
    user_id: &str,
    since: &str,
    central_home: &str,
) -> Result<usize, String> {
    let res = client
        .get(format!("{}/rest/v1/records", url))
        .header("apikey", anon_key)
        .header("Authorization", format!("Bearer {}", jwt))
        .query(&[
            ("user_id", format!("eq.{}", user_id)),
            ("updated_at", format!("gte.{}", since)),
            ("is_deleted", "eq.false".to_string()),
            ("select", "local_id,record_type,title,final_body,tags,source_url,key_insight,date,updated_at".to_string()),
        ])
        .send()
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status()));
    }

    let rows: Vec<Value> = res.json().map_err(|e| e.to_string())?;
    let mut imported = 0usize;

    for row in &rows {
        if let Err(e) = save_remote_record(row, central_home) {
            eprintln!("[Supabase] save_remote_record error: {}", e);
        } else {
            imported += 1;
        }
    }

    Ok(imported)
}

fn save_remote_record(row: &Value, central_home: &str) -> Result<(), String> {
    use std::fs;
    use crate::util::{record_dir_by_type, normalize_record_type, generate_filename};

    let id = row["local_id"].as_str().or_else(|| row["id"].as_str())
        .ok_or("missing id")?;
    let record_type = normalize_record_type(row["record_type"].as_str().unwrap_or("note"));
    let title = row["title"].as_str().unwrap_or("Untitled").to_string();
    let body = row["final_body"].as_str().unwrap_or("").to_string();
    let tags: Vec<String> = row["tags"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let date = row["date"].as_str().unwrap_or("").to_string();
    let source_url = row["source_url"].as_str().map(str::to_string);
    let updated_at = row["updated_at"].as_str().unwrap_or("").to_string();

    let dir = format!("{}/records/{}", central_home, record_dir_by_type(&record_type));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Check if file with this id already exists (any filename)
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<Value>(&content) {
                if existing["id"].as_str() == Some(id) {
                    // LWW: skip if local is same or newer
                    let local_ts = existing["created_at"].as_str().unwrap_or("");
                    if local_ts >= updated_at.as_str() { return Ok(()); }
                    // Overwrite with remote
                    let record_json = json!({
                        "id": id,
                        "record_type": record_type,
                        "title": title,
                        "content": body,
                        "tags": tags,
                        "source_url": source_url,
                        "date": date,
                        "created_at": updated_at,
                        "notion_sync_status": "SYNCED",
                    });
                    let bytes = serde_json::to_vec_pretty(&record_json).map_err(|e| e.to_string())?;
                    return crate::util::write_atomic(&path, &bytes).map_err(|e| e.to_string());
                }
            }
        }
    }

    // New record — create file
    let filename = generate_filename(&record_type, &title);
    let path = std::path::PathBuf::from(&dir).join(&filename);
    let record_json = json!({
        "id": id,
        "record_type": record_type,
        "title": title,
        "content": body,
        "tags": tags,
        "source_url": source_url,
        "date": date,
        "created_at": updated_at,
        "notion_sync_status": "SYNCED",
    });
    let bytes = serde_json::to_vec_pretty(&record_json).map_err(|e| e.to_string())?;
    crate::util::write_atomic(&path, &bytes).map_err(|e| e.to_string())
}
