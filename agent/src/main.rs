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
struct Config {
    // 공통
    storage_mode: StorageMode,
    local_path: String,
    
    // Cloud 모드 전용
    api_base: String,
    username: String,
    api_token: String,
    
    // Private Vault 모드 전용
    server_port: u16,
    server_token: String,
}

impl Default for Config {
    fn default() -> Self {
        let api_base = std::env::var("MDFLARE_API_BASE")
            .unwrap_or_else(|_| "https://mdflare.com".to_string());
        Self {
            storage_mode: StorageMode::Cloud,
            local_path: String::new(),
            api_base,
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
        let mut config = if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };
        // 환경변수가 있으면 항상 우선
        if let Ok(base) = std::env::var("MDFLARE_API_BASE") {
            config.api_base = base;
        }
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
        let resp: FilesResponse = self.client.get(&url).send()?.json()?;
        Ok(resp.files)
    }

    fn get_file(&self, path: &str) -> Result<FileContent, reqwest::Error> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client.get(&url).send()?.json()
    }

    fn put_file(&self, path: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&serde_json::json!({ "content": content }))
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

struct SyncEngine {
    api: ApiClient,
    local_path: PathBuf,
    local_hashes: HashMap<String, String>,
    remote_modified: HashMap<String, String>,
}

impl SyncEngine {
    fn new(config: &Config) -> Self {
        Self {
            api: ApiClient::new(&config.api_base, &config.username, &config.api_token),
            local_path: PathBuf::from(&config.local_path),
            local_hashes: HashMap::new(),
            remote_modified: HashMap::new(),
        }
    }

    fn simple_hash(s: &str) -> String {
        let mut hash: i32 = 0;
        for c in s.chars() {
            hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(c as i32);
        }
        format!("{:x}", hash)
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
                        println!("⬆️ {}", path);
                        uploaded += 1;
                    }
                    Err(e) => log::error!("파일 읽기 실패 {}: {}", path, e),
                }
            }
        }

        Ok((downloaded, uploaded))
    }

    fn handle_local_change(&mut self, full_path: &Path) {
        if let Ok(rel) = full_path.strip_prefix(&self.local_path) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            
            if full_path.exists() {
                if let Ok(content) = fs::read_to_string(full_path) {
                    let hash = Self::simple_hash(&content);
                    if self.local_hashes.get(&rel_str) != Some(&hash) {
                        self.local_hashes.insert(rel_str.clone(), hash);
                        if self.api.put_file(&rel_str, &content).is_ok() {
                            println!("⬆️ {}", rel_str);
                        }
                    }
                }
            } else {
                if self.api.delete_file(&rel_str).is_ok() {
                    self.local_hashes.remove(&rel_str);
                    println!("🗑️ {}", rel_str);
                }
            }
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
    let log_path = dirs::document_dir()
        .map(|d| d.join("mdflare-agent.log"))
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

fn load_icon() -> Icon {
    let rgba: Vec<u8> = (0..16*16).flat_map(|_| vec![255u8, 100, 50, 255]).collect();
    Icon::from_rgba(rgba, 16, 16).expect("Failed to create icon")
}

fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        path.replace(&home.to_string_lossy().to_string(), "~")
    } else {
        path.to_string()
    }
}

fn run_cloud_tray_app(config: Config) {
    let event_loop = EventLoop::new();
    
    let menu = Menu::new();
    
    let mode_item = MenuItem::new("☁️ Cloud 모드", false, None);
    let user_item = MenuItem::new(format!("👤 {}", config.username), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let sync_item = MenuItem::new("🔄 지금 동기화", true, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let web_item = MenuItem::new("🌐 웹에서 열기", true, None);
    let quit_item = MenuItem::new("종료", true, None);
    
    menu.append(&mode_item).ok();
    menu.append(&user_item).ok();
    menu.append(&path_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&sync_item).ok();
    menu.append(&folder_item).ok();
    menu.append(&web_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit_item).ok();
    
    let sync_id = sync_item.id().clone();
    let folder_id = folder_item.id().clone();
    let web_id = web_item.id().clone();
    let quit_id = quit_item.id().clone();
    
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent (Cloud)")
        .with_icon(load_icon())
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
                    }
                }
            }
        }
    });
    
    // 주기적 동기화
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
        .with_icon(load_icon())
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

fn build_cloud_menu(config: &Config) -> (Menu, muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId) {
    let menu = Menu::new();
    let mode_item = MenuItem::new("☁️ Cloud 모드", false, None);
    let user_item = MenuItem::new(format!("👤 {}", config.username), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let sync_item = MenuItem::new("🔄 지금 동기화", true, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let web_item = MenuItem::new("🌐 웹에서 열기", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    let sync_id = sync_item.id().clone();
    let folder_id = folder_item.id().clone();
    let web_id = web_item.id().clone();
    let quit_id = quit_item.id().clone();

    menu.append(&mode_item).ok();
    menu.append(&user_item).ok();
    menu.append(&path_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&sync_item).ok();
    menu.append(&folder_item).ok();
    menu.append(&web_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit_item).ok();

    (menu, sync_id, folder_id, web_id, quit_id)
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
                    }
                }
            }
        }
    });

    // 주기적 동기화
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

fn run_setup_tray_app() {
    let event_loop = EventLoop::new();

    let menu = Menu::new();
    let status_item = MenuItem::new("⚙️ 로그인 대기 중...", false, None);
    let login_item = MenuItem::new("🔐 브라우저에서 로그인", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    menu.append(&status_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&login_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit_item).ok();

    let login_id = login_item.id().clone();
    let quit_id = quit_item.id().clone();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent (설정 필요)")
        .with_icon(load_icon())
        .build()
        .expect("Failed to create tray icon");

    // 상태 공유: 콜백 후 cloud 모드 전환 정보
    let transitioned = Arc::new(Mutex::new(false));
    let cloud_state: Arc<Mutex<Option<(Config, Arc<Mutex<SyncEngine>>)>>> = Arc::new(Mutex::new(None));
    let cloud_menu_ids: Arc<Mutex<Option<(muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId)>>> = Arc::new(Mutex::new(None));

    let tray = std::cell::RefCell::new(tray);
    let transitioned_loop = transitioned.clone();
    let cloud_state_loop = cloud_state.clone();
    let cloud_menu_ids_loop = cloud_menu_ids.clone();

    let menu_receiver = MenuEvent::receiver();
    let transitioned_menu = transitioned.clone();
    let cloud_state_menu = cloud_state.clone();
    let cloud_menu_ids_menu = cloud_menu_ids.clone();

    thread::spawn(move || {
        loop {
            if let Ok(event) = menu_receiver.recv() {
                let is_cloud = *transitioned_menu.lock().unwrap();

                if !is_cloud {
                    // Setup 모드 메뉴
                    if event.id == login_id {
                        let config = Config::load();
                        let auth_url = format!("{}/auth/agent", config.api_base);
                        open::that(&auth_url).ok();
                    } else if event.id == quit_id {
                        std::process::exit(0);
                    }
                } else {
                    // Cloud 모드 메뉴
                    if let Some((sync_id, folder_id, web_id, quit_id)) = cloud_menu_ids_menu.lock().unwrap().as_ref() {
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
                        }
                    }
                }
            }
        }
    });

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::Opened { urls } = event {
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
                        config.local_path = dirs::document_dir()
                            .map(|d| d.join("MDFlare"))
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                    }
                    fs::create_dir_all(&config.local_path).ok();
                    config.save();

                    log_to_file(&format!("setup_tray: logged in as {} → switching to cloud tray", config.username));

                    // 트레이 메뉴를 Cloud 모드로 교체
                    let (cloud_menu, sync_id, folder_id, web_id, quit_id) = build_cloud_menu(&config);
                    tray.borrow_mut().set_menu(Some(Box::new(cloud_menu)));
                    tray.borrow_mut().set_tooltip(Some(&format!("MDFlare Agent (☁️ {})", config.username)));

                    // 동기화 엔진 시작
                    let engine = start_cloud_sync(&config);

                    *cloud_state_loop.lock().unwrap() = Some((config, engine));
                    *cloud_menu_ids_loop.lock().unwrap() = Some((sync_id, folder_id, web_id, quit_id));
                    *transitioned_loop.lock().unwrap() = true;
                }
            }
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

fn setup_cloud(config: Config) {
    let auth_url = format!("{}/auth/agent", config.api_base);
    println!("⚙️ Cloud 모드 - 브라우저에서 로그인하세요");
    println!("🌐 {}", auth_url);
    open::that(&auth_url).ok();
    run_setup_tray_app();
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

    // ── 첫 실행: 모드 선택 ──
    if !config.is_configured() {
        log_to_file("main: not configured → show mode selection");
        let choice = rfd::MessageDialog::new()
            .set_title("MDFlare Agent")
            .set_description(
                "사용할 모드를 선택하세요.\n\n\
                 ☁️ Cloud — mdflare.com 계정과 파일을 동기화\n\
                 🔐 Private Vault — 내 PC에 로컬 서버. 클라우드 없음"
            )
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                "☁️ Cloud".to_string(),
                "🔐 Private Vault".to_string(),
                "취소".to_string(),
            ))
            .show();

        let choice_str = match &choice {
            rfd::MessageDialogResult::Yes => "cloud",
            rfd::MessageDialogResult::No => "vault",
            rfd::MessageDialogResult::Custom(s) if s.contains("Cloud") => "cloud",
            rfd::MessageDialogResult::Custom(s) if s.contains("Vault") => "vault",
            _ => "",
        };

        if choice_str == "cloud" {
            let mut config = config;
            config.storage_mode = StorageMode::Cloud;
            config.save();
            setup_cloud(config);
        } else if choice_str == "vault" {
            setup_private_vault(config);
        }
        // else: 취소 → 종료
        return;
    }

    // ── 설정 완료: 저장된 모드로 시작 ──
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
