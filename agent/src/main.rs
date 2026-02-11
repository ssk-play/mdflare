mod config;
mod types;
mod cloud;
mod local_fs;
mod private_vault;
mod sync;
mod tray;

use config::{Config, StorageMode};
use tray::{
    handle_url_callback, log_to_file, register_url_scheme,
    run_cloud_tray_app, run_private_vault_tray_app, run_setup_tray_app,
    setup_private_vault,
};

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
