use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use directories::ProjectDirs;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use serde::{Deserialize, Serialize};
use tao::event_loop::{ControlFlow, EventLoop};
use tray_icon::{Icon, TrayIconBuilder};

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    api_base: String,
    username: String,
    local_path: String,
    api_token: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base: "https://mdflare.com".to_string(),
            username: String::new(),
            local_path: String::new(),
            api_token: String::new(),
        }
    }
}

impl Config {
    fn is_configured(&self) -> bool {
        !self.username.is_empty() && !self.local_path.is_empty() && !self.api_token.is_empty()
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
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    fn save(&self) {
        let path = Self::config_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(path, data).ok();
        }
    }
}

// ============================================================================
// API Client
// ============================================================================

#[derive(Debug, Deserialize)]
struct FileItem {
    #[allow(dead_code)]
    name: String,
    path: String,
    #[serde(rename = "type")]
    file_type: String,
    #[allow(dead_code)]
    size: Option<u64>,
    modified: Option<String>,
    children: Option<Vec<FileItem>>,
}

#[derive(Debug, Deserialize)]
struct FilesResponse {
    #[allow(dead_code)]
    user: String,
    files: Vec<FileItem>,
}

#[derive(Debug, Deserialize)]
struct FileContent {
    #[allow(dead_code)]
    path: String,
    content: String,
    #[allow(dead_code)]
    size: u64,
    #[allow(dead_code)]
    modified: String,
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
// Sync Engine
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
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&self.local_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "md" {
                        if let Ok(rel) = entry.path().strip_prefix(&self.local_path) {
                            files.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        }
        files
    }

    fn full_sync(&mut self) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let mut downloaded = 0;
        let mut uploaded = 0;

        // 서버 파일 목록
        let remote_files = self.api.list_files()?;
        let remote_items = Self::flatten_files(&remote_files);
        let remote_paths: Vec<String> = remote_items.iter().map(|(p, _)| p.clone()).collect();

        // 로컬 파일 목록
        let local_paths = self.scan_local_md_files();

        // 서버 → 로컬 (다운로드: 새 파일 또는 변경된 파일)
        for (path, modified) in &remote_items {
            let local_file = self.local_path.join(path);
            let should_download = if !local_file.exists() {
                true
            } else if let Some(mod_time) = modified {
                // 서버 파일이 변경됐는지 확인
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

        // 로컬 → 서버 (업로드: 새 파일)
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
                // 파일 수정/생성
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
                // 파일 삭제
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
fn register_url_scheme() {
    // macOS/Linux: handled differently
}

// ============================================================================
// Tray App
// ============================================================================

fn load_icon() -> Icon {
    // 간단한 16x16 빨간 아이콘 (🔥 대체)
    let rgba: Vec<u8> = (0..16*16).flat_map(|_| vec![255u8, 100, 50, 255]).collect();
    Icon::from_rgba(rgba, 16, 16).expect("Failed to create icon")
}

fn run_tray_app(config: Config) {
    let event_loop = EventLoop::new();
    
    // 메뉴 생성
    let menu = Menu::new();
    
    let user_item = MenuItem::new(format!("👤 {}", config.username), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let sync_item = MenuItem::new("🔄 지금 동기화", true, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let web_item = MenuItem::new("🌐 웹에서 열기", true, None);
    let quit_item = MenuItem::new("종료", true, None);
    
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
    
    // 트레이 아이콘
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent")
        .with_icon(load_icon())
        .build()
        .expect("Failed to create tray icon");
    
    // 동기화 엔진 (스레드 공유)
    let engine = Arc::new(Mutex::new(SyncEngine::new(&config)));
    let engine_clone = engine.clone();
    let local_path = config.local_path.clone();
    
    // 파일 감시 스레드
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
    
    // 주기적 동기화 스레드 (30초)
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
    
    // 메뉴 이벤트 핸들러
    let menu_channel = MenuEvent::receiver();
    let config_for_menu = config.clone();
    
    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        
        if let Ok(event) = menu_channel.try_recv() {
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
                *control_flow = ControlFlow::Exit;
            }
        }
        
        thread::sleep(Duration::from_millis(100));
    });
}

fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        path.replace(&home.to_string_lossy().to_string(), "~")
    } else {
        path.to_string()
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    env_logger::init();
    
    let args: Vec<String> = std::env::args().collect();
    
    // OAuth 콜백 처리
    if args.len() > 1 && args[1].starts_with("mdflare://") {
        if let Some((username, token)) = parse_oauth_callback(&args[1]) {
            println!("🎉 로그인 성공: {}", username);
            
            let mut config = Config::load();
            config.username = username;
            config.api_token = token;
            
            // 기본 폴더 설정 (없으면)
            if config.local_path.is_empty() {
                if let Some(docs) = dirs::document_dir() {
                    config.local_path = docs.join("MDFlare").to_string_lossy().to_string();
                }
            }
            
            // 폴더 생성
            fs::create_dir_all(&config.local_path).ok();
            
            config.save();
            println!("📁 동기화 폴더: {}", config.local_path);
            
            // 계속 실행
            run_tray_app(config);
        } else {
            eprintln!("❌ 잘못된 콜백 URL");
        }
        return;
    }
    
    // URL scheme 등록 (Windows)
    register_url_scheme();
    
    // 설정 로드
    let config = Config::load();
    
    if config.is_configured() {
        println!("✅ MDFlare Agent 시작");
        println!("👤 {}", config.username);
        println!("📁 {}", config.local_path);
        run_tray_app(config);
    } else {
        println!("⚙️ 설정 필요 - 브라우저에서 로그인하세요");
        open::that("https://mdflare.com/auth/agent").ok();
        
        // 설정 대기 (콜백으로 다시 실행됨)
        println!("브라우저에서 로그인 완료 후 앱이 자동 설정됩니다.");
    }
}
