use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use axum::{
    extract::{Path as AxumPath, State},
    http::{header, Method, StatusCode},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};

use crate::config::{Config, generate_connection_token, generate_connection_token_with_url};
use crate::types::{FileContent, FilesResponse, PutFileRequest};
use crate::local_fs::scan_local_md_files;

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
    headers: axum::http::HeaderMap,
) -> Result<Json<FilesResponse>, StatusCode> {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    let files = scan_local_md_files(&state.local_path);
    Ok(Json(FilesResponse {
        user: "local".to_string(),
        files,
    }))
}

async fn api_get_file(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    AxumPath(path): AxumPath<String>,
) -> Result<Json<FileContent>, StatusCode> {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    let decoded = urlencoding::decode(&path).map(|s| s.into_owned()).unwrap_or(path);
    let file_path = state.local_path.join(&decoded);
    
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
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    
    let decoded = urlencoding::decode(&path).map(|s| s.into_owned()).unwrap_or(path.clone());
    let file_path = state.local_path.join(&decoded);
    
    if !file_path.starts_with(&state.local_path) {
        return Err(StatusCode::FORBIDDEN);
    }
    
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
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    
    let decoded = urlencoding::decode(&path).map(|s| s.into_owned()).unwrap_or(path.clone());
    let file_path = state.local_path.join(&decoded);
    
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
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    check_auth(&state, auth).await?;
    
    let old_decoded = urlencoding::decode(&body.old_path).map(|s| s.into_owned()).unwrap_or(body.old_path.clone());
    let new_decoded = urlencoding::decode(&body.new_path).map(|s| s.into_owned()).unwrap_or(body.new_path.clone());
    
    let old_file_path = state.local_path.join(&old_decoded);
    let new_file_path = state.local_path.join(&new_decoded);
    
    if !old_file_path.starts_with(&state.local_path) || !new_file_path.starts_with(&state.local_path) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    if !old_file_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    if let Some(parent) = new_file_path.parent() {
        fs::create_dir_all(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    
    fs::rename(&old_file_path, &new_file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(serde_json::json!({
        "renamed": true,
        "oldPath": old_decoded,
        "newPath": new_decoded
    })))
}

pub async fn run_private_vault_server(config: Config) {
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
    
    let local_token = generate_connection_token(config.server_port, &config.server_token);
    println!("🔐 Private Vault 서버 시작: http://localhost:{}", config.server_port);
    println!("🔑 로컬 연결 토큰: {}", local_token);
    
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
    
    let url = loop {
        if let Some(line) = reader.next_line().await? {
            if line.contains("trycloudflare.com") {
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
    
    tokio::spawn(async move {
        while let Ok(Some(_)) = reader.next_line().await {}
        let _ = child.wait().await;
    });
    
    Ok((url, external_token))
}
