use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use axum::{
    extract::{Path as AxumPath, State},
    http::{header, Method, StatusCode},
    routing::get,
    Json, Router,
};
use directories::ProjectDirs;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use serde::{Deserialize, Serialize};
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop};
use tower_http::cors::{Any, CorsLayer};
use tray_icon::{Icon, TrayIconBuilder};

// ============================================================================
// Storage Mode
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StorageMode {
    Cloud,
    PrivateVault,
}

impl Default for StorageMode {
    fn default() -> Self {
        StorageMode::Cloud
    }
}

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerSettings {
    api_base: String,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            api_base: "https://mdflare.com".to_string(),
        }
    }
}

impl ServerSettings {
    fn settings_path() -> PathBuf {
        let proj = ProjectDirs::from("com", "mdflare", "agent")
            .expect("Failed to get config directory");
        let dir = proj.config_dir();
        fs::create_dir_all(dir).ok();
        dir.join("server_settings.json")
    }

    fn load() -> Self {
        let path = Self::settings_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    fn save(&self) {
        let path = Self::settings_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(path, data).ok();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    // 공통
    storage_mode: StorageMode,
    local_path: String,

    // Cloud 모드 전용 (api_base는 server_settings.json에서 로드)
    #[serde(skip)]
    api_base: String,
    username: String,
    api_token: String,

    // Private Vault 모드 전용
    server_port: u16,
    server_token: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            storage_mode: StorageMode::Cloud,
            local_path: String::new(),
            api_base: String::new(),
            username: String::new(),
            api_token: String::new(),
            server_port: 7779,
            server_token: generate_token(),
        }
    }
}

fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{:x}{:x}", now.as_secs(), now.subsec_nanos())
}

// 연결 토큰 생성: base64(serverUrl|token)
fn generate_connection_token(port: u16, token: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let plain = format!("http://localhost:{}|{}", port, token);
    STANDARD.encode(plain.as_bytes())
}

impl Config {
    fn is_configured(&self) -> bool {
        match self.storage_mode {
            StorageMode::Cloud => {
                !self.username.is_empty() && !self.local_path.is_empty() && !self.api_token.is_empty()
            }
            StorageMode::PrivateVault => {
                !self.local_path.is_empty()
            }
        }
    }

    fn config_path() -> PathBuf {
        let proj = ProjectDirs::from("com", "mdflare", "agent")
            .expect("Failed to get config directory");
        let dir = proj.config_dir();
        fs::create_dir_all(dir).ok();
        dir.join("config.json")
    }

    fn load() -> Self {
        let path = Self::config_path();
        let mut config: Self = if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };
        config.api_base = ServerSettings::load().api_base;
        config
    }

    fn save(&self) {
        let path = Self::config_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(path, data).ok();
        }
    }
}

// ============================================================================
// API Client (Cloud 모드용)
// ============================================================================

#[derive(Debug, Deserialize, Serialize, Clone)]
struct FileItem {
    name: String,
    path: String,
    #[serde(rename = "type")]
    file_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<FileItem>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FilesResponse {
    user: String,
    files: Vec<FileItem>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FileContent {
    path: String,
    content: String,
    size: u64,
    modified: String,
}

#[derive(Debug, Deserialize)]
struct PutFileRequest {
    content: String,
}

struct ApiClient {
    client: reqwest::blocking::Client,
    base_url: String,
    username: String,
    token: String,
}

impl ApiClient {
    fn new(base_url: &str, username: &str, token: &str) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            username: username.to_string(),
            token: token.to_string(),
        }
    }

    fn list_files(&self) -> Result<Vec<FileItem>, reqwest::Error> {
        let url = format!("{}/api/{}/files", self.base_url, self.username);
        let resp: FilesResponse = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?
            .json()?;
        Ok(resp.files)
    }

    fn get_file(&self, path: &str) -> Result<FileContent, reqwest::Error> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?
            .json()
    }

    fn put_file(&self, path: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.put_file_with_diff(path, content, None, None)
    }

    fn put_file_with_diff(
        &self,
        path: &str,
        content: &str,
        old_hash: Option<&str>,
        diff: Option<&serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        let mut body = serde_json::json!({ "content": content });
        if let Some(oh) = old_hash {
            body["oldHash"] = serde_json::json!(oh);
        }
        if let Some(d) = diff {
            body["diff"] = d.clone();
        }
        self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()?;
        Ok(())
    }

    fn delete_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?;
        Ok(())
    }

    fn put_heartbeat(&self) {
        let url = format!("{}/api/{}/agent-status", self.base_url, self.username);
        self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .ok();
    }

    fn get_sync_config(&self) -> Result<RtdbConfig, Box<dyn std::error::Error>> {
        let url = format!("{}/api/{}/sync-config", self.base_url, self.username);
        let resp: RtdbConfig = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?
            .json()?;
        Ok(resp)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RtdbConfig {
    rtdb_url: String,
    rtdb_auth: String,
    user_id: String,
}

// ============================================================================
// Local File System Helpers
// ============================================================================

fn scan_local_md_files(local_path: &Path) -> Vec<FileItem> {
    fn scan_dir(dir: &Path, base: &Path) -> Vec<FileItem> {
        let mut items = Vec::new();
        
        if let Ok(entries) = fs::read_dir(dir) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                
                // 숨김 파일/폴더 스킵
                if name.starts_with('.') {
                    continue;
                }
                
                if path.is_dir() {
                    let children = scan_dir(&path, base);
                    if !children.is_empty() || has_md_files(&path) {
                        let rel_path = path.strip_prefix(base).unwrap_or(&path);
                        items.push(FileItem {
                            name,
                            path: rel_path.to_string_lossy().replace('\\', "/"),
                            file_type: "folder".to_string(),
                            size: None,
                            modified: None,
                            children: Some(children),
                        });
                    }
                } else if path.extension().map_or(false, |e| e == "md") {
                    let rel_path = path.strip_prefix(base).unwrap_or(&path);
                    let metadata = fs::metadata(&path).ok();
                    items.push(FileItem {
                        name,
                        path: rel_path.to_string_lossy().replace('\\', "/"),
                        file_type: "file".to_string(),
                        size: metadata.as_ref().map(|m| m.len()),
                        modified: metadata.and_then(|m| {
                            m.modified().ok().map(|t| {
                                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                                datetime.to_rfc3339()
                            })
                        }),
                        children: None,
                    });
                }
            }
        }
        
        // 폴더 먼저, 그 다음 파일
        items.sort_by(|a, b| {
            match (&a.file_type[..], &b.file_type[..]) {
                ("folder", "file") => std::cmp::Ordering::Less,
                ("file", "folder") => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        
        items
    }
    
    fn has_md_files(dir: &Path) -> bool {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                    return true;
                }
                if path.is_dir() && has_md_files(&path) {
                    return true;
                }
            }
        }
        false
    }
    
    scan_dir(local_path, local_path)
}

fn flatten_file_paths(items: &[FileItem]) -> Vec<String> {
    let mut result = Vec::new();
    for item in items {
        if item.file_type == "folder" {
            if let Some(children) = &item.children {
                result.extend(flatten_file_paths(children));
            }
        } else {
            result.push(item.path.clone());
        }
    }
    result
}

// ============================================================================
// Private Vault Server
// ============================================================================

#[derive(Clone)]
struct ServerState {
    local_path: PathBuf,
    token: String,
}

async fn check_auth(
    state: &ServerState,
    auth_header: Option<&str>,
) -> Result<(), StatusCode> {
    match auth_header {
        Some(h) if h.starts_with("Bearer ") => {
            let token = &h[7..];
            if token == state.token {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn api_list_files(
    State(state): State<ServerState>,
) -> Result<Json<FilesResponse>, StatusCode> {
    let files = scan_local_md_files(&state.local_path);
    Ok(Json(FilesResponse {
        user: "local".to_string(),
        files,
    }))
}

async fn api_get_file(
    State(state): State<ServerState>,
    AxumPath(path): AxumPath<String>,
) -> Result<Json<FileContent>, StatusCode> {
    let decoded = urlencoding::decode(&path).map(|s| s.into_owned()).unwrap_or(path);
    let file_path = state.local_path.join(&decoded);
    
    // 보안: local_path 밖으로 나가지 못하게
    if !file_path.starts_with(&state.local_path) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let content = fs::read_to_string(&file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    let metadata = fs::metadata(&file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    
    let modified: chrono::DateTime<chrono::Utc> = metadata.modified()
        .map(|t| t.into())
        .unwrap_or_else(|_| chrono::Utc::now());
    
    Ok(Json(FileContent {
        path: decoded.to_string(),
        content: content.clone(),
        size: content.len() as u64,
        modified: modified.to_rfc3339(),
    }))
}

async fn api_put_file(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    AxumPath(path): AxumPath<String>,
    Json(body): Json<PutFileRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 인증 체크
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    
    let decoded = urlencoding::decode(&path).map(|s| s.into_owned()).unwrap_or(path.clone());
    let file_path = state.local_path.join(&decoded);
    
    // 보안: local_path 밖으로 나가지 못하게
    if !file_path.starts_with(&state.local_path) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // 상위 폴더 생성
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    
    fs::write(&file_path, &body.content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(serde_json::json!({
        "path": decoded.to_string(),
        "saved": true,
        "size": body.content.len()
    })))
}

async fn api_delete_file(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    AxumPath(path): AxumPath<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 인증 체크
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    
    let decoded = urlencoding::decode(&path).map(|s| s.into_owned()).unwrap_or(path.clone());
    let file_path = state.local_path.join(&decoded);
    
    // 보안: local_path 밖으로 나가지 못하게
    if !file_path.starts_with(&state.local_path) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    if file_path.is_dir() {
        fs::remove_dir_all(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        fs::remove_file(&file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    }
    
    Ok(Json(serde_json::json!({
        "path": decoded.to_string(),
        "deleted": true
    })))
}

#[derive(Deserialize)]
struct RenameRequest {
    #[serde(rename = "oldPath")]
    old_path: String,
    #[serde(rename = "newPath")]
    new_path: String,
}

async fn api_rename(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<RenameRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 인증 체크
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    
    let old_decoded = urlencoding::decode(&body.old_path).map(|s| s.into_owned()).unwrap_or(body.old_path.clone());
    let new_decoded = urlencoding::decode(&body.new_path).map(|s| s.into_owned()).unwrap_or(body.new_path.clone());
    
    let old_file_path = state.local_path.join(&old_decoded);
    let new_file_path = state.local_path.join(&new_decoded);
    
    // 보안: local_path 밖으로 나가지 못하게
    if !old_file_path.starts_with(&state.local_path) || !new_file_path.starts_with(&state.local_path) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // 원본 파일/폴더 존재 확인
    if !old_file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    // 상위 폴더 생성
    if let Some(parent) = new_file_path.parent() {
        fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    
    // 이름 변경 (파일/폴더 모두 지원)
    fs::rename(&old_file_path, &new_file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(serde_json::json!({
        "renamed": true,
        "oldPath": old_decoded,
        "newPath": new_decoded
    })))
}

async fn run_private_vault_server(config: Config) {
    let state = ServerState {
        local_path: PathBuf::from(&config.local_path),
        token: config.server_token.clone(),
    };
    
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
    
    let app = Router::new()
        .route("/api/files", get(api_list_files))
        .route("/api/file/*path", get(api_get_file).put(api_put_file).delete(api_delete_file))
        .route("/api/rename", axum::routing::post(api_rename))
        .layer(cors)
        .with_state(state);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    
    // 로컬 연결 토큰
    let local_token = generate_connection_token(config.server_port, &config.server_token);
    println!("🔐 Private Vault 서버 시작: http://localhost:{}", config.server_port);
    println!("🔑 로컬 연결 토큰: {}", local_token);
    
    // localtunnel 터널 시작 (외부 접속용)
    let server_token = config.server_token.clone();
    tokio::spawn(async move {
        match start_tunnel(config.server_port, &server_token).await {
            Ok((url, external_token)) => {
                println!("🌍 외부 접속: {}", url);
                println!("🔑 외부 연결 토큰: {}", external_token);
            }
            Err(e) => {
                println!("⚠️ 터널 연결 실패 (로컬만 사용): {}", e);
            }
        }
    });
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// cloudflared Quick Tunnel 시작
async fn start_tunnel(local_port: u16, token: &str) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    use std::process::Stdio;
    use tokio::process::Command;
    use tokio::io::{BufReader, AsyncBufReadExt};
    
    let mut child = Command::new("cloudflared")
        .args(["tunnel", "--url", &format!("http://localhost:{}", local_port)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    
    let stderr = child.stderr.take().ok_or("stderr 없음")?;
    let mut reader = BufReader::new(stderr).lines();
    
    // URL 파싱 (stderr에서 trycloudflare.com URL 찾기)
    let url = loop {
        if let Some(line) = reader.next_line().await? {
            if line.contains("trycloudflare.com") {
                // URL 추출: https://xxx.trycloudflare.com
                if let Some(start) = line.find("https://") {
                    let url_part = &line[start..];
                    if let Some(end) = url_part.find(|c: char| c.is_whitespace() || c == '|') {
                        break url_part[..end].to_string();
                    } else {
                        break url_part.trim().to_string();
                    }
                }
            }
        } else {
            return Err("cloudflared URL을 받지 못함".into());
        }
    };
    
    let external_token = generate_connection_token_with_url(&url, token);
    
    // 프로세스 유지 (백그라운드) - stderr 계속 읽어서 drain
    tokio::spawn(async move {
        // stderr를 계속 읽어서 프로세스가 block되지 않도록 함
        while let Ok(Some(_)) = reader.next_line().await {}
        let _ = child.wait().await;
    });
    
    Ok((url, external_token))
}

// URL 기반 연결 토큰 생성
fn generate_connection_token_with_url(url: &str, token: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let plain = format!("{}|{}", url, token);
    STANDARD.encode(plain.as_bytes())
}

// ============================================================================
// Sync Engine (Cloud 모드용)
// ============================================================================

// ============================================================================
// RTDB types and diff helpers
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RtdbFileEntry {
    path: String,
    action: String,
    #[allow(dead_code)]
    hash: Option<String>,
    old_hash: Option<String>,
    diff: Option<Vec<serde_json::Value>>,
    old_path: Option<String>,
    #[allow(dead_code)]
    modified: Option<u64>,
    #[allow(dead_code)]
    size: Option<u64>,
}

/// Apply a line-based diff to content.
/// diff ops: {"eq": N}, {"del": N}, {"ins": ["line1", ...]}
fn apply_line_diff(old_content: &str, diff: &[serde_json::Value]) -> Option<String> {
    let old_lines: Vec<&str> = old_content.split('\n').collect();
    let mut result = Vec::new();
    let mut pos = 0;

    for op in diff {
        if let Some(eq) = op.get("eq").and_then(|v| v.as_u64()) {
            let eq = eq as usize;
            if pos + eq > old_lines.len() {
                return None; // diff doesn't match
            }
            result.extend_from_slice(&old_lines[pos..pos + eq]);
            pos += eq;
        } else if let Some(del) = op.get("del").and_then(|v| v.as_u64()) {
            let del = del as usize;
            if pos + del > old_lines.len() {
                return None;
            }
            pos += del;
        } else if let Some(ins) = op.get("ins").and_then(|v| v.as_array()) {
            for line in ins {
                if let Some(s) = line.as_str() {
                    result.push(s);
                } else {
                    return None;
                }
            }
        } else {
            return None; // unknown op
        }
    }
    // remaining lines (if any eq ops missed)
    result.extend_from_slice(&old_lines[pos..]);
    Some(result.join("\n"))
}

/// Generate a line-based diff using the `similar` crate.
fn generate_line_diff(old_content: &str, new_content: &str) -> serde_json::Value {
    use similar::{ChangeTag, TextDiff};

    let text_diff = TextDiff::from_lines(old_content, new_content);
    let mut ops: Vec<serde_json::Value> = Vec::new();
    let mut eq_count = 0usize;
    let mut del_count = 0usize;
    let mut ins_lines: Vec<String> = Vec::new();

    let flush_del = |ops: &mut Vec<serde_json::Value>, del: &mut usize| {
        if *del > 0 {
            ops.push(serde_json::json!({"del": *del}));
            *del = 0;
        }
    };
    let flush_ins = |ops: &mut Vec<serde_json::Value>, ins: &mut Vec<String>| {
        if !ins.is_empty() {
            ops.push(serde_json::json!({"ins": ins.clone()}));
            ins.clear();
        }
    };
    let flush_eq = |ops: &mut Vec<serde_json::Value>, eq: &mut usize| {
        if *eq > 0 {
            ops.push(serde_json::json!({"eq": *eq}));
            *eq = 0;
        }
    };

    for change in text_diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                flush_del(&mut ops, &mut del_count);
                flush_ins(&mut ops, &mut ins_lines);
                eq_count += 1;
            }
            ChangeTag::Delete => {
                flush_eq(&mut ops, &mut eq_count);
                flush_ins(&mut ops, &mut ins_lines);
                del_count += 1;
            }
            ChangeTag::Insert => {
                flush_eq(&mut ops, &mut eq_count);
                // strip trailing newline that similar adds
                let val = change.value();
                let line = if val.ends_with('\n') { &val[..val.len()-1] } else { val };
                ins_lines.push(line.to_string());
            }
        }
    }
    flush_eq(&mut ops, &mut eq_count);
    flush_del(&mut ops, &mut del_count);
    flush_ins(&mut ops, &mut ins_lines);

    serde_json::json!(ops)
}

/// Convert i32 to base-36 string, matching JS `Number.prototype.toString(36)`.
/// Negative numbers are prefixed with '-'.
fn to_base36(n: i32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let negative = n < 0;
    let mut val = if negative { (n as i64).abs() as u64 } else { n as u64 };
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while val > 0 {
        buf.push(digits[(val % 36) as usize]);
        val /= 36;
    }
    if negative {
        buf.push(b'-');
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

struct SyncEngine {
    api: ApiClient,
    local_path: PathBuf,
    local_hashes: HashMap<String, String>,
    local_content_cache: HashMap<String, String>,
    remote_modified: HashMap<String, String>,
}

impl SyncEngine {
    fn new(config: &Config) -> Self {
        Self {
            api: ApiClient::new(&config.api_base, &config.username, &config.api_token),
            local_path: PathBuf::from(&config.local_path),
            local_hashes: HashMap::new(),
            local_content_cache: HashMap::new(),
            remote_modified: HashMap::new(),
        }
    }

    fn simple_hash(s: &str) -> String {
        let mut hash: i32 = 0;
        for c in s.chars() {
            hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(c as i32);
        }
        // JS의 hash.toString(36)과 동일한 base-36 출력
        to_base36(hash)
    }

    fn flatten_files(items: &[FileItem]) -> Vec<(String, Option<String>)> {
        let mut result = Vec::new();
        for item in items {
            if item.file_type == "folder" {
                if let Some(children) = &item.children {
                    result.extend(Self::flatten_files(children));
                }
            } else if item.file_type == "file" {
                result.push((item.path.clone(), item.modified.clone()));
            }
        }
        result
    }

    fn scan_local_md_files(&self) -> Vec<String> {
        flatten_file_paths(&scan_local_md_files(&self.local_path))
    }

    fn full_sync(&mut self) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let mut downloaded = 0;
        let mut uploaded = 0;

        let remote_files = self.api.list_files()?;
        let remote_items = Self::flatten_files(&remote_files);
        let remote_paths: Vec<String> = remote_items.iter().map(|(p, _)| p.clone()).collect();

        let local_paths = self.scan_local_md_files();

        // 서버 → 로컬
        for (path, modified) in &remote_items {
            let local_file = self.local_path.join(path);
            let should_download = if !local_file.exists() {
                true
            } else if let Some(mod_time) = modified {
                self.remote_modified.get(path) != Some(mod_time)
            } else {
                false
            };

            if should_download {
                match self.api.get_file(path) {
                    Ok(content) => {
                        if let Some(parent) = local_file.parent() {
                            fs::create_dir_all(parent).ok();
                        }
                        if let Err(e) = fs::write(&local_file, &content.content) {
                            log::error!("파일 쓰기 실패 {}: {}", path, e);
                            continue;
                        }
                        self.local_hashes.insert(path.clone(), Self::simple_hash(&content.content));
                        self.local_content_cache.insert(path.clone(), content.content);
                        if let Some(mod_time) = modified {
                            self.remote_modified.insert(path.clone(), mod_time.clone());
                        }
                        println!("⬇️ {}", path);
                        downloaded += 1;
                    }
                    Err(e) => log::error!("파일 다운로드 실패 {}: {}", path, e),
                }
            }
        }

        // 로컬 → 서버
        for path in &local_paths {
            if !remote_paths.contains(path) {
                let local_file = self.local_path.join(path);
                match fs::read_to_string(&local_file) {
                    Ok(content) => {
                        if let Err(e) = self.api.put_file(path, &content) {
                            log::error!("파일 업로드 실패 {}: {}", path, e);
                            continue;
                        }
                        self.local_hashes.insert(path.clone(), Self::simple_hash(&content));
                        self.local_content_cache.insert(path.clone(), content);
                        println!("⬆️ {}", path);
                        uploaded += 1;
                    }
                    Err(e) => log::error!("파일 읽기 실패 {}: {}", path, e),
                }
            }
        }

        self.api.put_heartbeat();
        Ok((downloaded, uploaded))
    }

    fn handle_local_change(&mut self, full_path: &Path) {
        if let Ok(rel) = full_path.strip_prefix(&self.local_path) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            if full_path.exists() {
                if let Ok(content) = fs::read_to_string(full_path) {
                    let new_hash = Self::simple_hash(&content);
                    if self.local_hashes.get(&rel_str) != Some(&new_hash) {
                        let old_hash = self.local_hashes.get(&rel_str).cloned();
                        // 이전 내용 읽어서 diff 생성 (해시가 있으면 이전 버전 존재)
                        let diff = if old_hash.is_some() {
                            let diff_val = generate_line_diff(
                                &self.local_content_cache.get(&rel_str).map(|s| s.as_str()).unwrap_or(""),
                                &content,
                            );
                            let diff_str = diff_val.to_string();
                            if diff_str.len() <= 10240 { Some(diff_val) } else { None }
                        } else {
                            None
                        };
                        self.local_hashes.insert(rel_str.clone(), new_hash);
                        self.local_content_cache.insert(rel_str.clone(), content.clone());
                        let result = self.api.put_file_with_diff(
                            &rel_str,
                            &content,
                            old_hash.as_deref(),
                            diff.as_ref(),
                        );
                        if result.is_ok() {
                            println!("⬆️ {}", rel_str);
                        }
                    }
                }
            } else {
                if self.api.delete_file(&rel_str).is_ok() {
                    self.local_hashes.remove(&rel_str);
                    self.local_content_cache.remove(&rel_str);
                    println!("🗑️ {}", rel_str);
                }
            }
        }
    }

    fn handle_local_folder_delete(&mut self, folder_path: &Path) {
        if let Ok(rel) = folder_path.strip_prefix(&self.local_path) {
            let prefix = rel.to_string_lossy().replace('\\', "/");
            let prefix_with_slash = if prefix.ends_with('/') { prefix.clone() } else { format!("{}/", prefix) };
            let to_delete: Vec<String> = self.local_hashes.keys()
                .filter(|k| k.starts_with(&prefix_with_slash))
                .cloned()
                .collect();
            for path in to_delete {
                if self.api.delete_file(&path).is_ok() {
                    self.local_hashes.remove(&path);
                    self.local_content_cache.remove(&path);
                    println!("🗑️ {}", path);
                }
            }
        }
    }

    /// Handle an RTDB event (from SSE subscription)
    fn handle_rtdb_event(&mut self, entry: &RtdbFileEntry) {
        match entry.action.as_str() {
            "save" => {
                let local_file = self.local_path.join(&entry.path);
                let local_hash = self.local_hashes.get(&entry.path).cloned();

                // diff 적용 가능: 로컬 해시 == oldHash
                if let (Some(old_hash), Some(diff), Some(ref lh)) = (&entry.old_hash, &entry.diff, &local_hash) {
                    if lh == old_hash {
                        if let Ok(old_content) = fs::read_to_string(&local_file) {
                            if let Some(new_content) = apply_line_diff(&old_content, diff) {
                                if let Some(parent) = local_file.parent() {
                                    fs::create_dir_all(parent).ok();
                                }
                                if fs::write(&local_file, &new_content).is_ok() {
                                    let hash = Self::simple_hash(&new_content);
                                    self.local_hashes.insert(entry.path.clone(), hash);
                                    self.local_content_cache.insert(entry.path.clone(), new_content);
                                    println!("⬇️ {} (diff applied)", entry.path);
                                    return;
                                }
                            }
                        }
                    }
                }

                // fallback: R2에서 전체 파일 fetch
                self.fetch_from_r2(&entry.path);
            }
            "create" => {
                self.fetch_from_r2(&entry.path);
            }
            "delete" => {
                let local_file = self.local_path.join(&entry.path);
                if local_file.exists() {
                    if fs::remove_file(&local_file).is_ok() {
                        self.local_hashes.remove(&entry.path);
                        self.local_content_cache.remove(&entry.path);
                        println!("🗑️ {} (rtdb)", entry.path);
                    }
                }
            }
            "rename" => {
                if let Some(old_path) = &entry.old_path {
                    let old_file = self.local_path.join(old_path);
                    let new_file = self.local_path.join(&entry.path);
                    if old_file.exists() {
                        if let Some(parent) = new_file.parent() {
                            fs::create_dir_all(parent).ok();
                        }
                        if fs::rename(&old_file, &new_file).is_ok() {
                            // 해시 이전
                            if let Some(h) = self.local_hashes.remove(old_path) {
                                self.local_hashes.insert(entry.path.clone(), h);
                            }
                            if let Some(c) = self.local_content_cache.remove(old_path) {
                                self.local_content_cache.insert(entry.path.clone(), c);
                            }
                            println!("📝 {} → {} (rtdb)", old_path, entry.path);
                        }
                    } else {
                        // 이전 파일 없으면 R2에서 fetch
                        self.fetch_from_r2(&entry.path);
                    }
                }
            }
            _ => {}
        }
    }

    fn fetch_from_r2(&mut self, path: &str) {
        match self.api.get_file(path) {
            Ok(content) => {
                let local_file = self.local_path.join(path);
                if let Some(parent) = local_file.parent() {
                    fs::create_dir_all(parent).ok();
                }
                if fs::write(&local_file, &content.content).is_ok() {
                    self.local_hashes.insert(path.to_string(), Self::simple_hash(&content.content));
                    self.local_content_cache.insert(path.to_string(), content.content);
                    println!("⬇️ {} (r2)", path);
                }
            }
            Err(e) => log::error!("R2 fetch 실패 {}: {}", path, e),
        }
    }
}

// ============================================================================
// URL Scheme Handler
// ============================================================================

fn parse_oauth_callback(url_str: &str) -> Option<(String, String)> {
    let url = url::Url::parse(url_str).ok()?;
    if url.host_str() != Some("callback") {
        return None;
    }

    let params: HashMap<_, _> = url.query_pairs().collect();
    let username = params.get("username")?.to_string();
    let token = params.get("token")?.to_string();

    Some((username, token))
}

fn log_to_file(msg: &str) {
    use std::io::Write;
    let log_path = ProjectDirs::from("com", "mdflare", "agent")
        .map(|p| {
            let dir = p.config_dir().to_path_buf();
            fs::create_dir_all(&dir).ok();
            dir.join("agent.log")
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/mdflare-agent.log"));
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let now = chrono::Local::now().format("%H:%M:%S%.3f");
        writeln!(f, "[{}] {}", now, msg).ok();
    }
}

fn handle_url_callback(url: &str) -> bool {
    log_to_file(&format!("handle_url_callback: {}", url));

    if !url.starts_with("mdflare://") {
        log_to_file("  → not mdflare:// scheme, skip");
        return false;
    }
    if let Some((username, token)) = parse_oauth_callback(url) {
        // 이미 같은 토큰이 저장되어 있으면 스킵 (재시작 시 URL 재전달 방지)
        let existing = Config::load();
        log_to_file(&format!("  → existing token: [{}...]", &existing.api_token.get(..16).unwrap_or("empty")));
        log_to_file(&format!("  → new token:      [{}...]", &token.get(..16).unwrap_or("empty")));

        if existing.api_token == token {
            log_to_file("  → SKIP: same token already saved");
            return true;
        }

        log_to_file(&format!("  → login success: {}", username));

        let mut config = existing;
        config.storage_mode = StorageMode::Cloud;
        config.username = username;
        config.api_token = token;

        if config.local_path.is_empty() {
            config.local_path = dirs::document_dir()
                .map(|d| d.join("MDFlare"))
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
        }

        fs::create_dir_all(&config.local_path).ok();
        config.save();
        log_to_file(&format!("  → config saved: {} ({})", config.username, config.local_path));

        // 2초 딜레이 후 재시작 (URL 재전달 방지)
        log_to_file("  → scheduling delayed restart");
        std::process::Command::new("sh")
            .args(["-c", "sleep 2 && open -a 'MDFlare Agent'"])
            .spawn()
            .ok();

        log_to_file("  → exiting");
        std::process::exit(0);
    }
    log_to_file("  → parse_oauth_callback returned None");
    false
}

#[cfg(windows)]
fn register_url_scheme() {
    use winreg::enums::*;
    use winreg::RegKey;

    if let Ok(exe_path) = std::env::current_exe() {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok((key, _)) = hkcu.create_subkey("Software\\Classes\\mdflare") {
            key.set_value("", &"URL:MDFlare Protocol").ok();
            key.set_value("URL Protocol", &"").ok();
            
            if let Ok((cmd_key, _)) = key.create_subkey("shell\\open\\command") {
                let cmd = format!("\"{}\" \"%1\"", exe_path.display());
                cmd_key.set_value("", &cmd).ok();
            }
        }
    }
}

#[cfg(not(windows))]
fn register_url_scheme() {}

// ============================================================================
// Tray App (Cloud 모드)
// ============================================================================

fn load_icon_active() -> Icon {
    // 🔥 불꽃 아이콘 (22x22) - MDFlare 트레이드마크
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let fx = x as f32;
            let fy = y as f32;

            // 불꽃 외곽 (주황-빨강)
            let outer = {
                let main = (fx - 11.0).powi(2) / 30.0 + (fy - 12.0).powi(2) / 55.0 < 1.0;
                let top = (fx - 11.0).powi(2) / 12.0 + (fy - 4.0).powi(2) / 18.0 < 1.0;
                let left_flick = (fx - 7.0).powi(2) / 8.0 + (fy - 6.0).powi(2) / 12.0 < 1.0;
                let right_flick = (fx - 15.0).powi(2) / 6.0 + (fy - 7.0).powi(2) / 10.0 < 1.0;
                (main || top || left_flick || right_flick) && fy > 2.0
            };

            // 불꽃 내부 (노랑)
            let inner = {
                let core = (fx - 11.0).powi(2) / 12.0 + (fy - 13.0).powi(2) / 20.0 < 1.0;
                let tip = (fx - 11.0).powi(2) / 5.0 + (fy - 8.0).powi(2) / 10.0 < 1.0;
                (core || tip) && fy > 6.0
            };

            if inner {
                rgba[idx] = 255;
                rgba[idx + 1] = 220;
                rgba[idx + 2] = 50;
                rgba[idx + 3] = 255;
            } else if outer {
                rgba[idx] = 255;
                rgba[idx + 1] = 90;
                rgba[idx + 2] = 20;
                rgba[idx + 3] = 255;
            }
        }
    }

    Icon::from_rgba(rgba, size, size).expect("Failed to create icon")
}

fn load_icon_setup() -> Icon {
    // 구름 + 금지 표시 (22x22)
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            let cx = 11.0f32;
            let cy = 11.0f32;
            let dist = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();

            // 구름 shape: 원 3개 합집합
            let is_cloud = {
                let main = (fx - 11.0).powi(2) + (fy - 13.0).powi(2) < 49.0;
                let top_l = (fx - 8.0).powi(2) + (fy - 9.0).powi(2) < 20.0;
                let top_r = (fx - 14.5).powi(2) + (fy - 10.0).powi(2) < 12.0;
                main || top_l || top_r
            };

            // 금지 원형 테두리 (두께 ~2px)
            let is_circle = dist >= 9.0 && dist <= 11.0;

            // 대각선 (좌상→우하)
            let line_dist = (fy - fx).abs() / std::f32::consts::SQRT_2;
            let is_line = line_dist < 1.5 && dist < 9.0;

            if is_circle || is_line {
                rgba[idx] = 210;
                rgba[idx + 1] = 50;
                rgba[idx + 2] = 50;
                rgba[idx + 3] = 255;
            } else if is_cloud {
                rgba[idx] = 150;
                rgba[idx + 1] = 155;
                rgba[idx + 2] = 160;
                rgba[idx + 3] = 200;
            }
        }
    }

    Icon::from_rgba(rgba, size, size).expect("Failed to create setup icon")
}

fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        path.replace(&home.to_string_lossy().to_string(), "~")
    } else {
        path.to_string()
    }
}

fn append_about(menu: &Menu) {
    let about = MenuItem::new(
        format!("MDFlare v{} ({})", env!("CARGO_PKG_VERSION"), env!("BUILD_DATE")),
        false,
        None,
    );
    menu.append(&about).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
}

fn run_cloud_tray_app(config: Config) {
    let event_loop = EventLoop::new();
    
    let menu = Menu::new();
    append_about(&menu);

    let mode_item = MenuItem::new("☁️ Cloud 모드", false, None);
    let user_item = MenuItem::new(format!("👤 {}", config.username), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let sync_item = MenuItem::new("🔄 지금 동기화", true, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let web_item = MenuItem::new("🌐 웹에서 열기", true, None);
    let logoff_item = MenuItem::new("🚪 로그아웃", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    menu.append(&mode_item).ok();
    menu.append(&user_item).ok();
    menu.append(&path_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&sync_item).ok();
    menu.append(&folder_item).ok();
    menu.append(&web_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&logoff_item).ok();
    menu.append(&quit_item).ok();

    let sync_id = sync_item.id().clone();
    let folder_id = folder_item.id().clone();
    let web_id = web_item.id().clone();
    let logoff_id = logoff_item.id().clone();
    let quit_id = quit_item.id().clone();
    
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent (Cloud)")
        .with_icon(load_icon_active())
        .build()
        .expect("Failed to create tray icon");

    let engine = Arc::new(Mutex::new(SyncEngine::new(&config)));
    let engine_clone = engine.clone();
    let local_path = config.local_path.clone();
    
    // 파일 감시
    let engine_watcher = engine.clone();
    let watch_path = local_path.clone();
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_secs(1), tx).unwrap();
        debouncer.watcher().watch(Path::new(&watch_path), RecursiveMode::Recursive).ok();

        for events in rx.iter().flatten() {
            for event in events {
                if event.kind == DebouncedEventKind::Any {
                    if event.path.extension().map_or(false, |e| e == "md") {
                        if let Ok(mut eng) = engine_watcher.lock() {
                            eng.handle_local_change(&event.path);
                        }
                    } else if !event.path.exists() {
                        // 폴더 삭제 감지: 경로가 존재하지 않고 확장자가 없으면 폴더 삭제
                        if let Ok(mut eng) = engine_watcher.lock() {
                            eng.handle_local_folder_delete(&event.path);
                        }
                    }
                }
            }
        }
    });
    
    // RTDB SSE 구독 (실시간 변경 감지)
    let engine_rtdb = engine.clone();
    let config_for_rtdb = config.clone();
    thread::spawn(move || {
        let api = ApiClient::new(
            &config_for_rtdb.api_base,
            &config_for_rtdb.username,
            &config_for_rtdb.api_token,
        );
        match api.get_sync_config() {
            Ok(rtdb_config) => {
                println!("🔌 RTDB 접속 정보 수신: {}", rtdb_config.user_id);
                start_rtdb_subscription(
                    rtdb_config.rtdb_url,
                    rtdb_config.rtdb_auth,
                    rtdb_config.user_id,
                    engine_rtdb,
                );
            }
            Err(e) => {
                eprintln!("⚠️ RTDB 접속 정보 조회 실패: {} (폴링만 사용)", e);
            }
        }
    });

    // 주기적 동기화 (fallback)
    let engine_timer = engine.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            if let Ok(mut eng) = engine_timer.lock() {
                eng.full_sync().ok();
            }
        }
    });

    // 초기 동기화
    if let Ok(mut eng) = engine.lock() {
        match eng.full_sync() {
            Ok((d, u)) => println!("✅ 초기 동기화 완료: ⬇️{} ⬆️{}", d, u),
            Err(e) => eprintln!("❌ 동기화 실패: {}", e),
        }
    }

    let config_for_menu = config.clone();
    let menu_receiver = MenuEvent::receiver();
    
    thread::spawn(move || {
        loop {
            if let Ok(event) = menu_receiver.recv() {
                if event.id == sync_id {
                    if let Ok(mut eng) = engine_clone.lock() {
                        eng.full_sync().ok();
                    }
                } else if event.id == folder_id {
                    open::that(&config_for_menu.local_path).ok();
                } else if event.id == web_id {
                    let url = format!("{}/{}", config_for_menu.api_base, config_for_menu.username);
                    open::that(url).ok();
                } else if event.id == logoff_id {
                    let path = Config::config_path();
                    fs::remove_file(&path).ok();
                    log_to_file("cloud: logoff → config deleted, restarting");
                    let exe = std::env::current_exe().unwrap();
                    std::process::Command::new(exe).spawn().ok();
                    std::process::exit(0);
                } else if event.id == quit_id {
                    std::process::exit(0);
                }
            }
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::Opened { urls } = event {
            for url in urls {
                handle_url_callback(url.as_str());
            }
        }
    });
}

// ============================================================================
// Tray App (Private Vault 모드)
// ============================================================================

fn run_private_vault_tray_app(config: Config) {
    let event_loop = EventLoop::new();
    let connection_token = generate_connection_token(config.server_port, &config.server_token);
    
    let menu = Menu::new();
    append_about(&menu);

    let mode_item = MenuItem::new("🔐 Private Vault 모드", false, None);
    let port_item = MenuItem::new(format!("🌐 http://localhost:{}", config.server_port), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let copy_token_item = MenuItem::new("📋 연결 토큰 복사", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    menu.append(&mode_item).ok();
    menu.append(&port_item).ok();
    menu.append(&path_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&folder_item).ok();
    menu.append(&copy_token_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit_item).ok();
    
    let folder_id = folder_item.id().clone();
    let copy_token_id = copy_token_item.id().clone();
    let quit_id = quit_item.id().clone();
    
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent (Private Vault)")
        .with_icon(load_icon_active())
        .build()
        .expect("Failed to create tray icon");
    
    // HTTP 서버를 별도 스레드에서 실행
    let config_for_server = config.clone();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(run_private_vault_server(config_for_server));
    });
    
    let config_for_menu = config.clone();
    let connection_token_for_menu = connection_token.clone();
    let menu_receiver = MenuEvent::receiver();
    
    thread::spawn(move || {
        loop {
            if let Ok(event) = menu_receiver.recv() {
                if event.id == folder_id {
                    open::that(&config_for_menu.local_path).ok();
                } else if event.id == copy_token_id {
                    // 클립보드 복사는 플랫폼별로 다름
                    #[cfg(target_os = "macos")]
                    {
                        std::process::Command::new("pbcopy")
                            .stdin(std::process::Stdio::piped())
                            .spawn()
                            .and_then(|mut child| {
                                use std::io::Write;
                                if let Some(stdin) = child.stdin.as_mut() {
                                    stdin.write_all(connection_token_for_menu.as_bytes()).ok();
                                }
                                child.wait()
                            })
                            .ok();
                    }
                    #[cfg(target_os = "windows")]
                    {
                        std::process::Command::new("cmd")
                            .args(["/C", &format!("echo {}| clip", connection_token_for_menu)])
                            .spawn()
                            .ok();
                    }
                    println!("📋 연결 토큰이 클립보드에 복사되었습니다");
                } else if event.id == quit_id {
                    std::process::exit(0);
                }
            }
        }
    });
    
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::Opened { urls } = event {
            for url in urls {
                handle_url_callback(url.as_str());
            }
        }
    });
}

// ============================================================================
// Setup Tray App (미설정 상태)
// ============================================================================

fn build_cloud_menu(config: &Config) -> (Menu, muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId) {
    let menu = Menu::new();
    append_about(&menu);
    let mode_item = MenuItem::new("☁️ Cloud 모드", false, None);
    let user_item = MenuItem::new(format!("👤 {}", config.username), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let sync_item = MenuItem::new("🔄 지금 동기화", true, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let web_item = MenuItem::new("🌐 웹에서 열기", true, None);
    let logoff_item = MenuItem::new("🚪 로그아웃", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    let sync_id = sync_item.id().clone();
    let folder_id = folder_item.id().clone();
    let web_id = web_item.id().clone();
    let logoff_id = logoff_item.id().clone();
    let quit_id = quit_item.id().clone();

    menu.append(&mode_item).ok();
    menu.append(&user_item).ok();
    menu.append(&path_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&sync_item).ok();
    menu.append(&folder_item).ok();
    menu.append(&web_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&logoff_item).ok();
    menu.append(&quit_item).ok();

    (menu, sync_id, folder_id, web_id, logoff_id, quit_id)
}

/// Start RTDB SSE subscription in a background thread.
/// Parses Firebase REST SSE events and dispatches to SyncEngine.
fn start_rtdb_subscription(
    rtdb_url: String,
    rtdb_auth: String,
    username: String,
    engine: Arc<Mutex<SyncEngine>>,
) {
    thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(None)
            .build()
            .unwrap();

        loop {
            let url = format!(
                "{}/mdflare/{}/files.json?auth={}",
                rtdb_url, username, rtdb_auth
            );
            println!("🔌 RTDB SSE 연결 중...");

            let resp = client
                .get(&url)
                .header("Accept", "text/event-stream")
                .send();

            match resp {
                Ok(response) => {
                    use std::io::{BufRead, BufReader};
                    let reader = BufReader::new(response);
                    let mut event_type = String::new();
                    let mut data_buf = String::new();
                    let mut first_put = true; // 첫 "put"은 전체 스냅샷 (무시)

                    println!("✅ RTDB SSE 연결됨");

                    for line in reader.lines() {
                        match line {
                            Ok(line) => {
                                if line.starts_with("event:") {
                                    event_type = line[6..].trim().to_string();
                                } else if line.starts_with("data:") {
                                    data_buf = line[5..].trim().to_string();
                                } else if line.is_empty() && !event_type.is_empty() {
                                    // 이벤트 완료 → 처리
                                    if event_type == "put" || event_type == "patch" {
                                        if first_put && event_type == "put" {
                                            first_put = false;
                                            // 첫 put은 전체 스냅샷, 스킵
                                            event_type.clear();
                                            data_buf.clear();
                                            continue;
                                        }
                                        handle_sse_data(&data_buf, &engine);
                                    } else if event_type == "keep-alive" {
                                        // ignore
                                    }
                                    event_type.clear();
                                    data_buf.clear();
                                }
                            }
                            Err(e) => {
                                eprintln!("⚠️ RTDB SSE 읽기 오류: {}", e);
                                break;
                            }
                        }
                    }

                    eprintln!("⚠️ RTDB SSE 연결 끊어짐, 5초 후 재연결...");
                }
                Err(e) => {
                    eprintln!("⚠️ RTDB SSE 연결 실패: {}, 5초 후 재시도...", e);
                }
            }

            thread::sleep(Duration::from_secs(5));
        }
    });
}

/// Parse SSE data payload and dispatch to SyncEngine
fn handle_sse_data(data: &str, engine: &Arc<Mutex<SyncEngine>>) {
    // Firebase SSE data format: {"path":"/safeKey","data":{...}} or {"path":"/","data":{...}}
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
    let val = match parsed {
        Ok(v) => v,
        Err(_) => return,
    };

    let path = val.get("path").and_then(|p| p.as_str()).unwrap_or("");
    let data_val = match val.get("data") {
        Some(d) => d,
        None => return,
    };

    if path == "/" {
        // 루트 업데이트: 여러 파일 변경 가능 (각 키가 safeKey)
        if let Some(obj) = data_val.as_object() {
            for (_key, entry_val) in obj {
                if let Ok(entry) = serde_json::from_value::<RtdbFileEntry>(entry_val.clone()) {
                    if let Ok(mut eng) = engine.lock() {
                        eng.handle_rtdb_event(&entry);
                    }
                }
            }
        }
    } else {
        // 개별 파일 변경: path = "/safeKey"
        if data_val.is_null() {
            // 삭제 이벤트: safeKey → path 복원
            let safe_key = path.trim_start_matches('/');
            let file_path = safe_key
                .replace("_slash_", "/")
                .replace("_dot_", ".");
            let entry = RtdbFileEntry {
                path: file_path,
                action: "delete".to_string(),
                hash: None,
                old_hash: None,
                diff: None,
                old_path: None,
                modified: None,
                size: None,
            };
            if let Ok(mut eng) = engine.lock() {
                eng.handle_rtdb_event(&entry);
            }
        } else if let Ok(entry) = serde_json::from_value::<RtdbFileEntry>(data_val.clone()) {
            if let Ok(mut eng) = engine.lock() {
                eng.handle_rtdb_event(&entry);
            }
        }
    }
}

fn start_cloud_sync(config: &Config) -> Arc<Mutex<SyncEngine>> {
    let engine = Arc::new(Mutex::new(SyncEngine::new(config)));
    let local_path = config.local_path.clone();

    // 파일 감시
    let engine_watcher = engine.clone();
    let watch_path = local_path.clone();
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_secs(1), tx).unwrap();
        debouncer.watcher().watch(Path::new(&watch_path), RecursiveMode::Recursive).ok();
        for events in rx.iter().flatten() {
            for event in events {
                if event.kind == DebouncedEventKind::Any {
                    if event.path.extension().map_or(false, |e| e == "md") {
                        if let Ok(mut eng) = engine_watcher.lock() {
                            eng.handle_local_change(&event.path);
                        }
                    } else if !event.path.exists() {
                        if let Ok(mut eng) = engine_watcher.lock() {
                            eng.handle_local_folder_delete(&event.path);
                        }
                    }
                }
            }
        }
    });

    // RTDB SSE 구독 (실시간 변경 감지)
    let engine_rtdb = engine.clone();
    let config_for_rtdb = config.clone();
    thread::spawn(move || {
        // sync-config에서 RTDB 접속 정보 가져오기
        let api = ApiClient::new(
            &config_for_rtdb.api_base,
            &config_for_rtdb.username,
            &config_for_rtdb.api_token,
        );
        match api.get_sync_config() {
            Ok(rtdb_config) => {
                println!("🔌 RTDB 접속 정보 수신: {}", rtdb_config.user_id);
                start_rtdb_subscription(
                    rtdb_config.rtdb_url,
                    rtdb_config.rtdb_auth,
                    rtdb_config.user_id,
                    engine_rtdb,
                );
            }
            Err(e) => {
                eprintln!("⚠️ RTDB 접속 정보 조회 실패: {} (폴링만 사용)", e);
            }
        }
    });

    // 주기적 동기화 (fallback: RTDB 연결 끊김 대비)
    let engine_timer = engine.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            if let Ok(mut eng) = engine_timer.lock() {
                eng.full_sync().ok();
            }
        }
    });

    // 초기 동기화
    if let Ok(mut eng) = engine.lock() {
        match eng.full_sync() {
            Ok((d, u)) => println!("✅ 초기 동기화 완료: ⬇️{} ⬆️{}", d, u),
            Err(e) => eprintln!("❌ 동기화 실패: {}", e),
        }
    }

    engine
}

/// 앱 상태: setup → cloud_waiting → cloud / vault
#[derive(Debug, Clone, PartialEq)]
enum AppPhase {
    Setup,         // 미연결 - "동기화 시작" 메뉴 표시
    CloudWaiting,  // Cloud 선택 후 브라우저 로그인 대기
    Cloud,         // Cloud 동기화 중
    Vault,         // Private Vault 동작 중
}

const FOLDER_SELECTION_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;background:#f5f5f7;padding:32px 24px 24px;color:#1d1d1f;-webkit-user-select:none;user-select:none}
h1{font-size:18px;font-weight:600;text-align:center;margin-bottom:8px}
.desc{font-size:13px;color:#86868b;text-align:center;margin-bottom:20px;line-height:1.5}
.path-box{background:#fff;border:2px solid #0071e3;border-radius:10px;padding:12px 16px;font-size:14px;color:#1d1d1f;word-break:break-all;margin-bottom:16px;min-height:44px;display:flex;align-items:center}
.buttons{display:flex;flex-direction:column;gap:8px}
.btn{padding:10px;border-radius:10px;font-size:14px;font-weight:500;cursor:pointer;border:none;text-align:center}
.btn-primary{background:#0071e3;color:#fff}
.btn-primary:hover{background:#0077ED}
.btn-primary:active{transform:scale(.98)}
.btn-secondary{background:#e8e8ed;color:#1d1d1f}
.btn-secondary:hover{background:#dddde1}
.btn-cancel{background:none;color:#86868b;margin-top:4px}
.btn-cancel:hover{background:#e8e8ed}
</style></head><body>
<h1>동기화 폴더 선택</h1>
<p class="desc">마크다운 파일이 저장될 폴더를 확인하세요.</p>
<div class="path-box" id="path">DEFAULT_PATH</div>
<div class="buttons">
  <div class="btn btn-primary" onclick="confirm()">이 폴더로 시작</div>
  <div class="btn btn-secondary" onclick="browse()">다른 폴더 선택...</div>
  <div class="btn btn-cancel" onclick="cancel()">취소</div>
</div>
<script>
let currentPath = 'DEFAULT_PATH';
function confirm(){ window.ipc.postMessage('ok:' + currentPath) }
function browse(){ window.ipc.postMessage('browse:') }
function cancel(){ window.ipc.postMessage('cancel:') }
function setPath(p){ currentPath = p; document.getElementById('path').textContent = p; }
</script>
</body></html>"#;

const SERVER_SELECTION_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;background:#f5f5f7;padding:32px 24px 24px;color:#1d1d1f;-webkit-user-select:none;user-select:none}
h1{font-size:18px;font-weight:600;text-align:center;margin-bottom:20px}
.options{display:flex;flex-direction:column;gap:8px}
.opt{background:#fff;border-radius:10px;padding:12px 16px;cursor:pointer;border:2px solid transparent;transition:all .15s;box-shadow:0 1px 3px rgba(0,0,0,.08);display:flex;align-items:center;gap:10px}
.opt:hover{border-color:#0071e3}
.opt.active{border-color:#0071e3;background:#f0f7ff}
.opt-icon{font-size:20px}
.opt-label{font-size:14px;font-weight:500}
.opt-desc{font-size:11px;color:#86868b}
.custom-input{display:none;margin-top:12px}
.custom-input input{width:100%;padding:10px 12px;font-size:14px;border:1px solid #d2d2d7;border-radius:8px;outline:none;background:#fff}
.custom-input input:focus{border-color:#0071e3}
.btn{display:block;width:100%;margin-top:16px;padding:10px;border-radius:10px;font-size:14px;font-weight:500;cursor:pointer;border:none;background:#0071e3;color:#fff;text-align:center}
.btn:hover{background:#0077ED}
.btn:active{transform:scale(.98)}
.cancel{display:block;width:100%;margin-top:8px;padding:8px;background:none;border:none;color:#86868b;font-size:13px;cursor:pointer;border-radius:8px;text-align:center}
.cancel:hover{background:#e8e8ed}
</style></head><body>
<h1>서버 설정</h1>
<div class="options">
  <div class="opt active" onclick="select(0)" id="o0">
    <span class="opt-icon">🌐</span>
    <div><div class="opt-label">mdflare.com</div><div class="opt-desc">클라우드 서버 (기본)</div></div>
  </div>
  <div class="opt" onclick="select(1)" id="o1">
    <span class="opt-icon">🖥️</span>
    <div><div class="opt-label">localhost:3000</div><div class="opt-desc">로컬 개발 서버</div></div>
  </div>
  <div class="opt" onclick="select(2)" id="o2">
    <span class="opt-icon">⚙️</span>
    <div><div class="opt-label">직접 입력</div><div class="opt-desc">커스텀 서버 주소</div></div>
  </div>
</div>
<div class="custom-input" id="ci">
  <input type="text" id="cu" placeholder="https://example.com" spellcheck="false">
</div>
<div class="btn" onclick="confirm()">확인</div>
<div class="cancel" onclick="window.ipc.postMessage('cancel')">취소</div>
<script>
let sel=0;
const urls=['https://mdflare.com','http://localhost:3000',''];
function select(i){
  sel=i;
  document.querySelectorAll('.opt').forEach((e,j)=>{e.className=j===i?'opt active':'opt'});
  document.getElementById('ci').style.display=i===2?'block':'none';
  if(i===2)document.getElementById('cu').focus();
}
function confirm(){
  let url=sel===2?document.getElementById('cu').value.trim():urls[sel];
  if(!url)return;
  url=url.replace(/\/+$/,'');
  window.ipc.postMessage('server:'+url);
}
select(CURRENT_SEL);
</script>
</body></html>"#;

const MODE_SELECTION_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;background:#f5f5f7;padding:32px 24px 24px;color:#1d1d1f;-webkit-user-select:none;user-select:none}
h1{font-size:18px;font-weight:600;text-align:center;margin-bottom:20px}
.cards{display:flex;flex-direction:column;gap:12px}
.card{background:#fff;border-radius:12px;padding:16px 20px;cursor:pointer;border:2px solid transparent;transition:all .15s;box-shadow:0 1px 3px rgba(0,0,0,.08)}
.card:hover{border-color:#0071e3;box-shadow:0 2px 8px rgba(0,113,227,.15)}
.card:active{transform:scale(.98)}
.card-header{display:flex;align-items:center;gap:8px;margin-bottom:8px}
.card-icon{font-size:24px}
.card-title{font-size:15px;font-weight:600}
.card-desc{font-size:12px;color:#86868b;line-height:1.6}
.card.disabled{opacity:.45;cursor:default;pointer-events:none}
.badge{font-size:10px;background:#86868b;color:#fff;padding:1px 6px;border-radius:8px;margin-left:auto}
.cancel{display:block;width:100%;margin-top:16px;padding:8px;background:none;border:none;color:#86868b;font-size:13px;cursor:pointer;border-radius:8px;text-align:center}
.cancel:hover{background:#e8e8ed}
</style></head><body>
<h1>동기화 방식 선택</h1>
<div class="cards">
  <div class="card" onclick="choose('cloud')">
    <div class="card-header"><span class="card-icon">☁️</span><span class="card-title">Cloud</span></div>
    <div class="card-desc">온라인 저장소에 파일을 동기화합니다.<br>에이전트 PC가 꺼져 있어도 온라인에서 편집할 수 있습니다.</div>
  </div>
  <div class="card disabled">
    <div class="card-header"><span class="card-icon">🔐</span><span class="card-title">Private Vault</span><span class="badge">준비중</span></div>
    <div class="card-desc">파일을 내 PC에만 보관합니다. (온라인 저장소 미사용)<br>에이전트가 꺼지면 온라인 에디터를 이용할 수 없습니다.</div>
  </div>
</div>
<div class="cancel" onclick="choose('cancel')">취소</div>
<script>function choose(m){window.ipc.postMessage(m)}</script>
</body></html>"#;

fn run_setup_tray_app() {
    let event_loop = EventLoop::new();

    // 초기 메뉴: 미설정 상태
    let menu = Menu::new();
    append_about(&menu);
    let settings_for_menu = ServerSettings::load();
    let server_label = format!("🌐 {}", settings_for_menu.api_base.replace("https://", "").replace("http://", ""));
    let server_item = MenuItem::new(&server_label, true, None);
    let start_item = MenuItem::new("시작하기", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    menu.append(&server_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&start_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit_item).ok();

    let server_id = server_item.id().clone();
    let start_id = start_item.id().clone();
    let quit_id = quit_item.id().clone();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent")
        .with_icon(load_icon_setup())
        .build()
        .expect("Failed to create tray icon");

    let tray = std::cell::RefCell::new(tray);

    // 상태 공유
    let phase = Arc::new(Mutex::new(AppPhase::Setup));
    let cloud_state: Arc<Mutex<Option<(Config, Arc<Mutex<SyncEngine>>)>>> = Arc::new(Mutex::new(None));
    let cloud_menu_ids: Arc<Mutex<Option<(muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId)>>> = Arc::new(Mutex::new(None));
    let vault_menu_ids: Arc<Mutex<Option<(muda::MenuId, muda::MenuId, muda::MenuId)>>> = Arc::new(Mutex::new(None));
    let needs_show_mode_dialog: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let dialog_choice: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let needs_show_folder_dialog: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let folder_choice: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let pending_cloud_config: Arc<Mutex<Option<Config>>> = Arc::new(Mutex::new(None));
    let needs_show_server_dialog: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let server_choice: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let phase_loop = phase.clone();
    let cloud_state_loop = cloud_state.clone();
    let cloud_menu_ids_loop = cloud_menu_ids.clone();
    let vault_menu_ids_loop = vault_menu_ids.clone();

    let menu_receiver = MenuEvent::receiver();
    let phase_menu = phase.clone();
    let cloud_state_menu = cloud_state.clone();
    let cloud_menu_ids_menu = cloud_menu_ids.clone();
    let vault_menu_ids_menu = vault_menu_ids.clone();
    let needs_show_mode_dialog_menu = needs_show_mode_dialog.clone();
    let needs_show_server_dialog_menu = needs_show_server_dialog.clone();

    thread::spawn(move || {
        loop {
            if let Ok(event) = menu_receiver.recv() {
                let current_phase = phase_menu.lock().unwrap().clone();

                match current_phase {
                    AppPhase::Setup => {
                        if event.id == start_id {
                            *needs_show_mode_dialog_menu.lock().unwrap() = true;
                        } else if event.id == server_id {
                            *needs_show_server_dialog_menu.lock().unwrap() = true;
                        } else if event.id == quit_id {
                            std::process::exit(0);
                        }
                    }
                    AppPhase::CloudWaiting => {
                        if event.id == quit_id {
                            std::process::exit(0);
                        }
                    }
                    AppPhase::Cloud => {
                        if let Some((sync_id, folder_id, web_id, logoff_id, quit_id)) = cloud_menu_ids_menu.lock().unwrap().as_ref() {
                            if &event.id == quit_id {
                                std::process::exit(0);
                            } else if &event.id == sync_id {
                                if let Some((_, engine)) = cloud_state_menu.lock().unwrap().as_ref() {
                                    if let Ok(mut eng) = engine.lock() {
                                        eng.full_sync().ok();
                                    }
                                }
                            } else if &event.id == folder_id {
                                if let Some((config, _)) = cloud_state_menu.lock().unwrap().as_ref() {
                                    open::that(&config.local_path).ok();
                                }
                            } else if &event.id == web_id {
                                if let Some((config, _)) = cloud_state_menu.lock().unwrap().as_ref() {
                                    let url = format!("{}/{}", config.api_base, config.username);
                                    open::that(url).ok();
                                }
                            } else if &event.id == logoff_id {
                                let mut config = Config::load();
                                config.username.clear();
                                config.api_token.clear();
                                config.local_path.clear();
                                config.save();
                                log_to_file("cloud: logoff → credentials cleared, restarting");
                                let exe = std::env::current_exe().unwrap();
                                std::process::Command::new(exe).spawn().ok();
                                std::process::exit(0);
                            }
                        }
                    }
                    AppPhase::Vault => {
                        if let Some((folder_id, copy_token_id, quit_id)) = vault_menu_ids_menu.lock().unwrap().as_ref() {
                            if &event.id == quit_id {
                                std::process::exit(0);
                            } else if &event.id == folder_id {
                                if let Some((config, _)) = cloud_state_menu.lock().unwrap().as_ref() {
                                    open::that(&config.local_path).ok();
                                }
                            } else if &event.id == copy_token_id {
                                // 클립보드 복사
                                let config = Config::load();
                                let conn_token = generate_connection_token(config.server_port, &config.server_token);
                                #[cfg(target_os = "macos")]
                                {
                                    std::process::Command::new("pbcopy")
                                        .stdin(std::process::Stdio::piped())
                                        .spawn()
                                        .and_then(|mut child| {
                                            use std::io::Write;
                                            if let Some(stdin) = child.stdin.as_mut() {
                                                stdin.write_all(conn_token.as_bytes()).ok();
                                            }
                                            child.wait()
                                        })
                                        .ok();
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // 트레이 메뉴 업데이트 요청용 플래그
    let needs_cloud_update: Arc<Mutex<Option<Config>>> = Arc::new(Mutex::new(None));
    let needs_vault_update: Arc<Mutex<Option<Config>>> = Arc::new(Mutex::new(None));
    let needs_cloud_waiting_update: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let needs_cloud_update_loop = needs_cloud_update.clone();
    let needs_vault_update_loop = needs_vault_update.clone();
    let needs_cloud_waiting_update_loop = needs_cloud_waiting_update.clone();

    // phase 변경을 감지해서 tray 업데이트 플래그 세팅하는 감시 스레드
    let phase_watcher = phase.clone();
    let needs_vault_update_watcher = needs_vault_update.clone();
    let needs_cloud_waiting_update_watcher = needs_cloud_waiting_update.clone();
    thread::spawn(move || {
        let mut last_phase = AppPhase::Setup;
        loop {
            thread::sleep(Duration::from_millis(100));
            let current = phase_watcher.lock().unwrap().clone();
            if current != last_phase {
                match &current {
                    AppPhase::CloudWaiting => {
                        *needs_cloud_waiting_update_watcher.lock().unwrap() = true;
                    }
                    AppPhase::Vault => {
                        let config = Config::load();
                        *needs_vault_update_watcher.lock().unwrap() = Some(config);
                    }
                    _ => {}
                }
                last_phase = current;
            }
        }
    });

    let needs_show_mode_dialog_loop = needs_show_mode_dialog.clone();
    let dialog_choice_loop = dialog_choice.clone();
    let needs_show_folder_dialog_loop = needs_show_folder_dialog.clone();
    let folder_choice_loop = folder_choice.clone();
    let pending_cloud_config_loop = pending_cloud_config.clone();
    let needs_show_server_dialog_loop = needs_show_server_dialog.clone();
    let server_choice_loop = server_choice.clone();
    let mut mode_dialog_webview: Option<wry::WebView> = None;
    let mut mode_dialog_window: Option<tao::window::Window> = None;
    let mut folder_dialog_webview: Option<wry::WebView> = None;
    let mut folder_dialog_window: Option<tao::window::Window> = None;
    let mut server_dialog_webview: Option<wry::WebView> = None;
    let mut server_dialog_window: Option<tao::window::Window> = None;

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + Duration::from_millis(100)
        );

        // 모드 선택 다이얼로그 표시
        {
            let mut flag = needs_show_mode_dialog_loop.lock().unwrap();
            if *flag {
                *flag = false;
                let window = tao::window::WindowBuilder::new()
                    .with_title("MDFlare")
                    .with_inner_size(tao::dpi::LogicalSize::new(420.0, 360.0))
                    .with_resizable(false)
                    .build(target)
                    .expect("Failed to create dialog window");

                let choice_clone = dialog_choice_loop.clone();
                let webview = wry::WebViewBuilder::new(&window)
                    .with_html(MODE_SELECTION_HTML)
                    .with_ipc_handler(move |req| {
                        *choice_clone.lock().unwrap() = Some(req.body().clone());
                    })
                    .build()
                    .expect("Failed to create webview");

                mode_dialog_window = Some(window);
                mode_dialog_webview = Some(webview);
            }
        }

        // 다이얼로그 선택 결과 처리
        if let Some(choice) = dialog_choice_loop.lock().unwrap().take() {
            mode_dialog_webview.take();
            mode_dialog_window.take();

            match choice.as_str() {
                "cloud" => {
                    let config = Config::load();
                    let auth_url = format!("{}/auth/agent", config.api_base);
                    open::that(&auth_url).ok();
                    *phase_loop.lock().unwrap() = AppPhase::CloudWaiting;
                    log_to_file("setup: cloud selected → waiting for browser login");
                }
                "vault" => {
                    let mut config = Config::load();
                    config.storage_mode = StorageMode::PrivateVault;
                    config.local_path = pick_folder("Private Vault 폴더 선택");
                    fs::create_dir_all(&config.local_path).ok();
                    config.save();
                    *phase_loop.lock().unwrap() = AppPhase::Vault;
                    log_to_file(&format!("setup: vault selected → {}", config.local_path));

                    let config_for_server = config.clone();
                    thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(run_private_vault_server(config_for_server));
                    });
                }
                _ => {} // cancel
            }
        }

        // 폴더 선택 다이얼로그 표시
        {
            let mut flag = needs_show_folder_dialog_loop.lock().unwrap();
            if *flag {
                *flag = false;
                let default_path = dirs::document_dir()
                    .map(|d| d.join("MDFlare"))
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let html = FOLDER_SELECTION_HTML.replace("DEFAULT_PATH", &default_path);

                let window = tao::window::WindowBuilder::new()
                    .with_title("MDFlare")
                    .with_inner_size(tao::dpi::LogicalSize::new(420.0, 320.0))
                    .with_resizable(false)
                    .build(target)
                    .expect("Failed to create folder dialog window");

                let fc = folder_choice_loop.clone();
                let webview = wry::WebViewBuilder::new(&window)
                    .with_html(&html)
                    .with_ipc_handler(move |req| {
                        *fc.lock().unwrap() = Some(req.body().clone());
                    })
                    .build()
                    .expect("Failed to create folder webview");

                folder_dialog_window = Some(window);
                folder_dialog_webview = Some(webview);
            }
        }

        // 폴더 선택 결과 처리
        if let Some(choice) = folder_choice_loop.lock().unwrap().take() {
            let action = if choice.starts_with("browse:") {
                "browse"
            } else if choice.starts_with("ok:") {
                "ok"
            } else {
                "cancel"
            };

            match action {
                "browse" => {
                    let selected = pick_folder("동기화 폴더 선택");
                    // 선택된 경로를 웹뷰에 전달
                    if let Some(ref wv) = folder_dialog_webview {
                        let js = format!("setPath('{}')", selected.replace('\\', "\\\\").replace('\'', "\\'"));
                        wv.evaluate_script(&js).ok();
                    }
                }
                "ok" => {
                    let path = choice.strip_prefix("ok:").unwrap_or("").to_string();
                    folder_dialog_webview.take();
                    folder_dialog_window.take();

                    if let Some(mut config) = pending_cloud_config_loop.lock().unwrap().take() {
                        config.local_path = path;
                        fs::create_dir_all(&config.local_path).ok();
                        config.save();

                        log_to_file(&format!("setup_tray: folder selected → {} → switching to cloud tray", config.local_path));

                        let (cloud_menu, sync_id, folder_id, web_id, logoff_id, quit_id) = build_cloud_menu(&config);
                        tray.borrow_mut().set_menu(Some(Box::new(cloud_menu)));
                        let _ = tray.borrow_mut().set_tooltip(Some(&format!("MDFlare Agent (☁️ {})", config.username)));
                        tray.borrow_mut().set_icon(Some(load_icon_active())).ok();

                        let engine = start_cloud_sync(&config);
                        *cloud_state_loop.lock().unwrap() = Some((config, engine));
                        *cloud_menu_ids_loop.lock().unwrap() = Some((sync_id, folder_id, web_id, logoff_id, quit_id));
                        *phase_loop.lock().unwrap() = AppPhase::Cloud;
                    }
                }
                _ => {
                    // cancel — 폴더 선택 취소, 다이얼로그 닫고 대기 상태 유지
                    folder_dialog_webview.take();
                    folder_dialog_window.take();
                    pending_cloud_config_loop.lock().unwrap().take();
                    *phase_loop.lock().unwrap() = AppPhase::Setup;
                }
            }
        }

        // 서버 설정 다이얼로그 표시
        {
            let mut flag = needs_show_server_dialog_loop.lock().unwrap();
            if *flag {
                *flag = false;
                let settings = ServerSettings::load();
                let current_sel = match settings.api_base.as_str() {
                    "https://mdflare.com" => 0,
                    "http://localhost:3000" => 1,
                    _ => 2,
                };
                let html = SERVER_SELECTION_HTML.replace("CURRENT_SEL", &current_sel.to_string());
                let html = if current_sel == 2 {
                    html.replace("https://example.com", &settings.api_base)
                } else {
                    html
                };

                let window = tao::window::WindowBuilder::new()
                    .with_title("MDFlare")
                    .with_inner_size(tao::dpi::LogicalSize::new(380.0, 380.0))
                    .with_resizable(false)
                    .build(target)
                    .expect("Failed to create server dialog window");

                let sc = server_choice_loop.clone();
                let webview = wry::WebViewBuilder::new(&window)
                    .with_html(&html)
                    .with_ipc_handler(move |req| {
                        *sc.lock().unwrap() = Some(req.body().clone());
                    })
                    .build()
                    .expect("Failed to create server webview");

                server_dialog_window = Some(window);
                server_dialog_webview = Some(webview);
            }
        }

        // 서버 설정 결과 처리
        if let Some(choice) = server_choice_loop.lock().unwrap().take() {
            server_dialog_webview.take();
            server_dialog_window.take();

            if let Some(url) = choice.strip_prefix("server:") {
                let mut settings = ServerSettings::load();
                settings.api_base = url.to_string();
                settings.save();
                log_to_file(&format!("setup: server changed → {}", url));

                let label = format!("🌐 {}", url.replace("https://", "").replace("http://", ""));
                server_item.set_text(&label);
            }
        }

        // 트레이 업데이트 폴링
        if let Some(config) = needs_cloud_update_loop.lock().unwrap().take() {
            let (cloud_menu, sync_id, folder_id, web_id, logoff_id, quit_id) = build_cloud_menu(&config);
            tray.borrow_mut().set_menu(Some(Box::new(cloud_menu)));
            let _ = tray.borrow_mut().set_tooltip(Some(&format!("MDFlare Agent (☁️ {})", config.username)));
            tray.borrow_mut().set_icon(Some(load_icon_active())).ok();

            let engine = start_cloud_sync(&config);
            *cloud_state_loop.lock().unwrap() = Some((config, engine));
            *cloud_menu_ids_loop.lock().unwrap() = Some((sync_id, folder_id, web_id, logoff_id, quit_id));
            *phase_loop.lock().unwrap() = AppPhase::Cloud;
        }

        if let Some(config) = needs_vault_update_loop.lock().unwrap().take() {
            let vault_menu = Menu::new();
            append_about(&vault_menu);
            let mode_item = MenuItem::new("🔐 Private Vault 모드", false, None);
            let port_item = MenuItem::new(format!("🌐 http://localhost:{}", config.server_port), false, None);
            let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
            let folder_item = MenuItem::new("📂 폴더 열기", true, None);
            let copy_token_item = MenuItem::new("📋 연결 토큰 복사", true, None);
            let quit_item = MenuItem::new("종료", true, None);

            let folder_id = folder_item.id().clone();
            let copy_token_id = copy_token_item.id().clone();
            let quit_id = quit_item.id().clone();

            vault_menu.append(&mode_item).ok();
            vault_menu.append(&port_item).ok();
            vault_menu.append(&path_item).ok();
            vault_menu.append(&PredefinedMenuItem::separator()).ok();
            vault_menu.append(&folder_item).ok();
            vault_menu.append(&copy_token_item).ok();
            vault_menu.append(&PredefinedMenuItem::separator()).ok();
            vault_menu.append(&quit_item).ok();

            tray.borrow_mut().set_menu(Some(Box::new(vault_menu)));
            let _ = tray.borrow_mut().set_tooltip(Some("MDFlare Agent (🔐 Private Vault)"));
            tray.borrow_mut().set_icon(Some(load_icon_active())).ok();

            *vault_menu_ids_loop.lock().unwrap() = Some((folder_id, copy_token_id, quit_id));
        }

        {
            let mut flag = needs_cloud_waiting_update_loop.lock().unwrap();
            if *flag {
                *flag = false;
                let waiting_menu = Menu::new();
                append_about(&waiting_menu);
                let status_item = MenuItem::new("☁️ 브라우저에서 로그인 중...", false, None);
                let quit_item = MenuItem::new("종료", true, None);
                waiting_menu.append(&status_item).ok();
                waiting_menu.append(&PredefinedMenuItem::separator()).ok();
                waiting_menu.append(&quit_item).ok();
                tray.borrow_mut().set_menu(Some(Box::new(waiting_menu)));
            }
        }

        // 이벤트 처리
        match event {
            Event::WindowEvent { event: tao::event::WindowEvent::CloseRequested, .. } => {
                // 다이얼로그 닫기 (X 버튼)
                mode_dialog_webview.take();
                mode_dialog_window.take();
                server_dialog_webview.take();
                server_dialog_window.take();
                if folder_dialog_webview.is_some() {
                    folder_dialog_webview.take();
                    folder_dialog_window.take();
                    pending_cloud_config_loop.lock().unwrap().take();
                    *phase_loop.lock().unwrap() = AppPhase::Setup;
                }
            }
            Event::Opened { urls } => {
                for url in urls {
                    let url_str = url.as_str();
                    if !url_str.starts_with("mdflare://") {
                        continue;
                    }
                    log_to_file(&format!("setup_tray: received URL {}", url_str));

                    if let Some((username, token)) = parse_oauth_callback(url_str) {
                        let existing = Config::load();
                        if existing.api_token == token {
                            log_to_file("setup_tray: duplicate token, skip");
                            continue;
                        }

                        let mut config = existing;
                        config.storage_mode = StorageMode::Cloud;
                        config.username = username;
                        config.api_token = token;

                        if config.local_path.is_empty() {
                            // 폴더 선택 다이얼로그 표시
                            log_to_file(&format!("setup_tray: logged in as {} → showing folder dialog", config.username));
                            *pending_cloud_config_loop.lock().unwrap() = Some(config);
                            *needs_show_folder_dialog_loop.lock().unwrap() = true;
                        } else {
                            // 이미 폴더가 설정된 경우 (재로그인 등)
                            fs::create_dir_all(&config.local_path).ok();
                            config.save();

                            log_to_file(&format!("setup_tray: logged in as {} → switching to cloud tray", config.username));

                            let (cloud_menu, sync_id, folder_id, web_id, logoff_id, quit_id) = build_cloud_menu(&config);
                            tray.borrow_mut().set_menu(Some(Box::new(cloud_menu)));
                            let _ = tray.borrow_mut().set_tooltip(Some(&format!("MDFlare Agent (☁️ {})", config.username)));
                            tray.borrow_mut().set_icon(Some(load_icon_active())).ok();

                            let engine = start_cloud_sync(&config);
                            *cloud_state_loop.lock().unwrap() = Some((config, engine));
                            *cloud_menu_ids_loop.lock().unwrap() = Some((sync_id, folder_id, web_id, logoff_id, quit_id));
                            *phase_loop.lock().unwrap() = AppPhase::Cloud;
                        }
                    }
                }
            }
            _ => {}
        }
    });
}

// ============================================================================
// Main
// ============================================================================

fn pick_folder(title: &str) -> String {
    let default_path = dirs::document_dir()
        .map(|d| d.join("MDFlare"))
        .unwrap_or_default();

    rfd::FileDialog::new()
        .set_title(title)
        .set_directory(&default_path)
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| default_path.to_string_lossy().to_string())
}

fn setup_private_vault(mut config: Config) {
    config.storage_mode = StorageMode::PrivateVault;
    config.local_path = pick_folder("Private Vault 폴더 선택");
    fs::create_dir_all(&config.local_path).ok();
    config.save();

    let conn_token = generate_connection_token(config.server_port, &config.server_token);
    println!("🔐 Private Vault 모드");
    println!("📁 {}", config.local_path);
    println!("🔑 연결 토큰: {}", conn_token);

    run_private_vault_tray_app(config);
}

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    // CLI 인자 처리
    if args.len() > 1 {
        match args[1].as_str() {
            "--private-vault" | "-p" => {
                let config = Config::load();
                setup_private_vault(config);
                return;
            }
            "--cloud" | "-c" => {
                let mut config = Config::load();
                config.storage_mode = StorageMode::Cloud;
                config.save();
                // 아래에서 처리
            }
            url if url.starts_with("mdflare://") => {
                handle_url_callback(url);
                return;
            }
            "--help" | "-h" => {
                println!("MDFlare Agent - 마크다운 동기화");
                println!();
                println!("사용법:");
                println!("  mdflare-agent              저장된 설정으로 시작");
                println!("  mdflare-agent -p           Private Vault 모드");
                println!("  mdflare-agent -c           Cloud 모드");
                println!("  -h, --help                 도움말");
                return;
            }
            _ => {}
        }
    }

    // Windows URL scheme 등록
    register_url_scheme();

    let config = Config::load();
    log_to_file(&format!("main: mode={:?} configured={} api_base={}", config.storage_mode, config.is_configured(), config.api_base));

    if !config.is_configured() {
        // 미설정 → 트레이에 미연결 아이콘 + "동기화 시작" 메뉴
        log_to_file("main: not configured → setup tray");
        run_setup_tray_app();
    } else {
        // 설정 완료 → 바로 동작
        log_to_file(&format!("main: configured → starting {:?} mode", config.storage_mode));
        match config.storage_mode {
            StorageMode::Cloud => {
                println!("☁️ Cloud 모드");
                println!("👤 {}", config.username);
                println!("📁 {}", config.local_path);
                run_cloud_tray_app(config);
            }
            StorageMode::PrivateVault => {
                println!("🔐 Private Vault 모드");
                println!("📁 {}", config.local_path);
                run_private_vault_tray_app(config);
            }
        }
    }
}
