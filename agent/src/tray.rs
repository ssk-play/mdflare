use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use directories::ProjectDirs;
use muda::{Menu, MenuItem, MenuId, PredefinedMenuItem, MenuEvent};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tao::event::{Event};
use tao::event_loop::{ControlFlow, EventLoop};
use tray_icon::{Icon, TrayIconBuilder};

use crate::config::{Config, StorageMode, ServerSettings, generate_connection_token};
use crate::cloud::ApiClient;
use crate::private_vault::run_private_vault_server;
use crate::sync::{SyncEngine, start_rtdb_subscription, start_cloud_sync};

// ============================================================================
// URL Scheme Handler
// ============================================================================

pub fn parse_oauth_callback(url_str: &str) -> Option<(String, String)> {
    let url = url::Url::parse(url_str).ok()?;
    if url.host_str() != Some("callback") {
        return None;
    }

    let params: HashMap<_, _> = url.query_pairs().collect();
    let username = params.get("username")?.to_string();
    let token = params.get("token")?.to_string();

    Some((username, token))
}

pub fn log_to_file(msg: &str) {
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

pub fn handle_url_callback(url: &str) -> bool {
    log_to_file(&format!("handle_url_callback: {}", url));

    if !url.starts_with("mdflare://") {
        log_to_file("  → not mdflare:// scheme, skip");
        return false;
    }
    if let Some((username, token)) = parse_oauth_callback(url) {
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
pub fn register_url_scheme() {
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
pub fn register_url_scheme() {}

// ============================================================================
// Icons
// ============================================================================

pub fn load_icon_active() -> Icon {
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let fx = x as f32;
            let fy = y as f32;

            let outer = {
                let main = (fx - 11.0).powi(2) / 30.0 + (fy - 12.0).powi(2) / 55.0 < 1.0;
                let top = (fx - 11.0).powi(2) / 12.0 + (fy - 4.0).powi(2) / 18.0 < 1.0;
                let left_flick = (fx - 7.0).powi(2) / 8.0 + (fy - 6.0).powi(2) / 12.0 < 1.0;
                let right_flick = (fx - 15.0).powi(2) / 6.0 + (fy - 7.0).powi(2) / 10.0 < 1.0;
                (main || top || left_flick || right_flick) && fy > 2.0
            };

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

pub fn load_icon_setup() -> Icon {
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

            let is_cloud = {
                let main = (fx - 11.0).powi(2) + (fy - 13.0).powi(2) < 49.0;
                let top_l = (fx - 8.0).powi(2) + (fy - 9.0).powi(2) < 20.0;
                let top_r = (fx - 14.5).powi(2) + (fy - 10.0).powi(2) < 12.0;
                main || top_l || top_r
            };

            let is_circle = dist >= 9.0 && dist <= 11.0;
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

// ============================================================================
// Utility functions
// ============================================================================

fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(text.as_bytes()).ok();
                }
                child.wait()
            })
            .map(|s| s.success())
            .unwrap_or(false);

        if ok {
            std::process::Command::new("osascript")
                .args(["-e", "display notification \"연결 토큰이 클립보드에 복사되었습니다\" with title \"MDFlare\""])
                .spawn()
                .ok();
        }
    }
}

fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        path.replace(&home.to_string_lossy().to_string(), "~")
    } else {
        path.to_string()
    }
}

fn version_string() -> String {
    if cfg!(debug_assertions) {
        format!("{}-d", env!("CARGO_PKG_VERSION"))
    } else {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

fn append_about(menu: &Menu) {
    let about = MenuItem::with_id(
        "about",
        format!("about {}", version_string()),
        true,
        None,
    );
    menu.append(&about).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
}

fn show_about_dialog() {
    rfd::MessageDialog::new()
        .set_title("MDFlare Agent")
        .set_description(&format!(
            "버전: {}\n빌드: {}\n\n\
            ⚠️ 현재 개발 중인 소프트웨어입니다.\n\
            데이터가 예고 없이 삭제될 수 있으며,\n\
            테스트 목적으로만 사용해 주세요.\n\n\
            본 소프트웨어 사용으로 인한 데이터 손실에 대해\n\
            개발자는 어떠한 책임도 지지 않습니다.",
            version_string(),
            env!("BUILD_DATE"),
        ))
        .set_level(rfd::MessageLevel::Info)
        .show();
}

pub fn pick_folder(title: &str) -> Option<String> {
    let default_path = dirs::document_dir()
        .map(|d| d.join("MDFlare"))
        .unwrap_or_default();

    rfd::FileDialog::new()
        .set_title(title)
        .set_directory(&default_path)
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
}

// ============================================================================
// Cloud Tray App
// ============================================================================

pub fn run_cloud_tray_app(config: Config) {
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

    let engine_timer = engine.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            if let Ok(mut eng) = engine_timer.lock() {
                eng.full_sync().ok();
            }
        }
    });

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
                if event.id == MenuId::new("about") {
                    show_about_dialog();
                } else if event.id == sync_id {
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
// Private Vault Tray App
// ============================================================================

pub fn run_private_vault_tray_app(config: Config) {
    let event_loop = EventLoop::new();

    let menu = Menu::new();
    append_about(&menu);

    let settings = ServerSettings::load();
    let web_label = settings.api_base.replace("https://", "").replace("http://", "");
    let mode_item = MenuItem::new("🔐 Private Vault 모드", false, None);
    let port_item = MenuItem::new(format!("🌐 {}", web_label), false, None);
    let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
    let folder_item = MenuItem::new("📂 폴더 열기", true, None);
    let web_item = MenuItem::new("🌐 웹페이지 열기", true, None);
    let copy_token_item = MenuItem::new("📋 연결 토큰 복사", true, None);
    let disconnect_item = MenuItem::new("🔌 연결 해제", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    menu.append(&mode_item).ok();
    menu.append(&port_item).ok();
    menu.append(&path_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&folder_item).ok();
    menu.append(&web_item).ok();
    menu.append(&copy_token_item).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&disconnect_item).ok();
    menu.append(&quit_item).ok();

    let folder_id = folder_item.id().clone();
    let web_id = web_item.id().clone();
    let copy_token_id = copy_token_item.id().clone();
    let disconnect_id = disconnect_item.id().clone();
    let quit_id = quit_item.id().clone();

    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MDFlare Agent (Private Vault)")
        .with_icon(load_icon_active())
        .build()
        .expect("Failed to create tray icon");

    let config_for_server = config.clone();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(run_private_vault_server(config_for_server));
    });

    let config_for_menu = config.clone();
    let menu_receiver = MenuEvent::receiver();

    thread::spawn(move || {
        loop {
            if let Ok(event) = menu_receiver.recv() {
                if event.id == MenuId::new("about") {
                    show_about_dialog();
                } else if event.id == folder_id {
                    open::that(&config_for_menu.local_path).ok();
                } else if event.id == web_id {
                    let settings = ServerSettings::load();
                    let conn_token = generate_connection_token(config_for_menu.server_port, &config_for_menu.server_token);
                    let url = format!("{}/?pvtoken={}", settings.api_base, urlencoding::encode(&conn_token));
                    open::that(url).ok();
                } else if event.id == copy_token_id {
                    let conn_token = generate_connection_token(config_for_menu.server_port, &config_for_menu.server_token);
                    copy_to_clipboard(&conn_token);
                } else if event.id == disconnect_id {
                    let mut config = Config::load();
                    config.local_path.clear();
                    config.server_token.clear();
                    config.save();
                    log_to_file("vault: disconnect → config cleared, restarting");
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

/// 앱 상태: setup → cloud_waiting → cloud / vault
#[derive(Debug, Clone, PartialEq)]
enum AppPhase {
    Setup,
    CloudWaiting,
    Cloud,
    Vault,
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
.notice{margin-bottom:16px;padding:10px 12px;background:#fff3cd;border-radius:8px;font-size:11px;color:#664d03;line-height:1.5}
</style></head><body>
<h1>동기화 방식 선택</h1>
<div class="notice">⚠️ 개발 중인 소프트웨어입니다. 데이터가 예고 없이 삭제될 수 있으며, 테스트 목적으로만 사용해 주세요. 데이터 손실에 대해 개발자는 책임지지 않습니다.</div>
<div class="cards">
  <div class="card" onclick="choose('cloud')">
    <div class="card-header"><span class="card-icon">☁️</span><span class="card-title">Cloud</span></div>
    <div class="card-desc">온라인 저장소에 파일을 동기화합니다.<br>에이전트 PC가 꺼져 있어도 온라인에서 편집할 수 있습니다.</div>
  </div>
  <div class="card" onclick="choose('vault')">
    <div class="card-header"><span class="card-icon">🔐</span><span class="card-title">Private Vault</span></div>
    <div class="card-desc">파일을 내 PC에만 보관합니다. (온라인 저장소 미사용)<br>에이전트가 꺼지면 온라인 에디터를 이용할 수 없습니다.</div>
  </div>
</div>
<div class="cancel" onclick="choose('cancel')">취소</div>
<script>function choose(m){window.ipc.postMessage(m)}</script>
</body></html>"#;

pub fn run_setup_tray_app() {
    let event_loop = EventLoop::new();

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

    let phase = Arc::new(Mutex::new(AppPhase::Setup));
    let cloud_state: Arc<Mutex<Option<(Config, Arc<Mutex<SyncEngine>>)>>> = Arc::new(Mutex::new(None));
    let cloud_menu_ids: Arc<Mutex<Option<(muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId)>>> = Arc::new(Mutex::new(None));
    let vault_menu_ids: Arc<Mutex<Option<(muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId, muda::MenuId)>>> = Arc::new(Mutex::new(None));
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
                if event.id == MenuId::new("about") {
                    show_about_dialog();
                    continue;
                }
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
                        if let Some((folder_id, web_id, copy_token_id, disconnect_id, quit_id)) = vault_menu_ids_menu.lock().unwrap().as_ref() {
                            if &event.id == quit_id {
                                std::process::exit(0);
                            } else if &event.id == folder_id {
                                if let Some((config, _)) = cloud_state_menu.lock().unwrap().as_ref() {
                                    open::that(&config.local_path).ok();
                                }
                            } else if &event.id == web_id {
                                let settings = ServerSettings::load();
                                let config = Config::load();
                                let conn_token = generate_connection_token(config.server_port, &config.server_token);
                                let url = format!("{}/?pvtoken={}", settings.api_base, urlencoding::encode(&conn_token));
                                open::that(url).ok();
                            } else if &event.id == copy_token_id {
                                let config = Config::load();
                                let conn_token = generate_connection_token(config.server_port, &config.server_token);
                                copy_to_clipboard(&conn_token);
                            } else if &event.id == disconnect_id {
                                let mut config = Config::load();
                                config.local_path.clear();
                                config.server_token.clear();
                                config.save();
                                log_to_file("vault: disconnect → config cleared, restarting");
                                let exe = std::env::current_exe().unwrap();
                                std::process::Command::new(exe).spawn().ok();
                                std::process::exit(0);
                            }
                        }
                    }
                }
            }
        }
    });

    let needs_cloud_update: Arc<Mutex<Option<Config>>> = Arc::new(Mutex::new(None));
    let needs_vault_update: Arc<Mutex<Option<Config>>> = Arc::new(Mutex::new(None));
    let needs_cloud_waiting_update: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let needs_cloud_update_loop = needs_cloud_update.clone();
    let needs_vault_update_loop = needs_vault_update.clone();
    let needs_cloud_waiting_update_loop = needs_cloud_waiting_update.clone();

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
                    if let Some(folder) = pick_folder("Private Vault 폴더 선택") {
                        let mut config = Config::load();
                        config.storage_mode = StorageMode::PrivateVault;
                        config.local_path = folder;
                        fs::create_dir_all(&config.local_path).ok();
                        config.save();
                        *phase_loop.lock().unwrap() = AppPhase::Vault;
                        log_to_file(&format!("setup: vault selected → {}", config.local_path));

                        let config_for_server = config.clone();
                        thread::spawn(move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(run_private_vault_server(config_for_server));
                        });

                        let settings = ServerSettings::load();
                        let conn_token = generate_connection_token(config.server_port, &config.server_token);
                        let web_url = format!("{}/?pvtoken={}", settings.api_base, urlencoding::encode(&conn_token));
                        thread::spawn(move || {
                            thread::sleep(Duration::from_millis(500));
                            open::that(web_url).ok();
                        });
                    } else {
                        log_to_file("setup: vault folder selection cancelled");
                    }
                }
                _ => {}
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
                    if let Some(selected) = pick_folder("동기화 폴더 선택") {
                        if let Some(ref wv) = folder_dialog_webview {
                            let js = format!("setPath('{}')", selected.replace('\\', "\\\\").replace('\'', "\\'"));
                            wv.evaluate_script(&js).ok();
                        }
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
            let settings = ServerSettings::load();
            let web_label = settings.api_base.replace("https://", "").replace("http://", "");
            let mode_item = MenuItem::new("🔐 Private Vault 모드", false, None);
            let port_item = MenuItem::new(format!("🌐 {}", web_label), false, None);
            let path_item = MenuItem::new(format!("📁 {}", shorten_path(&config.local_path)), false, None);
            let folder_item = MenuItem::new("📂 폴더 열기", true, None);
            let web_item = MenuItem::new("🌐 웹페이지 열기", true, None);
            let copy_token_item = MenuItem::new("📋 연결 토큰 복사", true, None);
            let disconnect_item = MenuItem::new("🔌 연결 해제", true, None);
            let quit_item = MenuItem::new("종료", true, None);

            let folder_id = folder_item.id().clone();
            let web_id = web_item.id().clone();
            let copy_token_id = copy_token_item.id().clone();
            let disconnect_id = disconnect_item.id().clone();
            let quit_id = quit_item.id().clone();

            vault_menu.append(&mode_item).ok();
            vault_menu.append(&port_item).ok();
            vault_menu.append(&path_item).ok();
            vault_menu.append(&PredefinedMenuItem::separator()).ok();
            vault_menu.append(&folder_item).ok();
            vault_menu.append(&web_item).ok();
            vault_menu.append(&copy_token_item).ok();
            vault_menu.append(&PredefinedMenuItem::separator()).ok();
            vault_menu.append(&disconnect_item).ok();
            vault_menu.append(&quit_item).ok();

            tray.borrow_mut().set_menu(Some(Box::new(vault_menu)));
            let _ = tray.borrow_mut().set_tooltip(Some("MDFlare Agent (🔐 Private Vault)"));
            tray.borrow_mut().set_icon(Some(load_icon_active())).ok();

            *vault_menu_ids_loop.lock().unwrap() = Some((folder_id, web_id, copy_token_id, disconnect_id, quit_id));
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
                            log_to_file(&format!("setup_tray: logged in as {} → showing folder dialog", config.username));
                            *pending_cloud_config_loop.lock().unwrap() = Some(config);
                            *needs_show_folder_dialog_loop.lock().unwrap() = true;
                        } else {
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

pub fn setup_private_vault(mut config: Config) {
    config.storage_mode = StorageMode::PrivateVault;
    if let Some(folder) = pick_folder("Private Vault 폴더 선택") {
        config.local_path = folder;
    } else {
        println!("폴더 선택이 취소되었습니다.");
        return;
    }
    fs::create_dir_all(&config.local_path).ok();
    config.save();

    let conn_token = generate_connection_token(config.server_port, &config.server_token);
    println!("🔐 Private Vault 모드");
    println!("📁 {}", config.local_path);
    println!("🔑 연결 토큰: {}", conn_token);

    run_private_vault_tray_app(config);
}