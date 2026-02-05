# MDFlare Windows Agent 개발 인수인계 문서 (Rust)

> 이 문서 하나로 Rust 기반 Windows 에이전트 개발을 바로 시작할 수 있습니다.

## 📁 프로젝트 구조

```
~/work/web/mdflare/
├── web/                    # 웹 프론트엔드 + API (Cloudflare Pages)
│   ├── src/                # React 앱
│   ├── functions/          # Cloudflare Pages Functions (API)
│   └── dist/               # 빌드 결과물
├── agent/                  # macOS 에이전트 (Swift, 참고용)
│   └── MDFlareAgent/
│       └── Sources/
│           └── main.swift  # 전체 코드 (단일 파일)
├── agent-rust/             # ← 새로 만들 Rust 에이전트
└── docs/                   # 문서
```

## 🌐 서비스 정보

- **웹사이트:** https://mdflare.com
- **API Base:** https://mdflare.com/api
- **GitHub:** https://github.com/ssk-play/mdflare

## 🦀 Rust 기술 스택

```toml
# Cargo.toml
[package]
name = "mdflare-agent"
version = "1.0.0"
edition = "2021"

[dependencies]
# HTTP 클라이언트
reqwest = { version = "0.11", features = ["json", "blocking"] }

# JSON 직렬화
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 시스템 트레이
tray-item = "0.10"          # 간단한 트레이 (Windows/macOS/Linux)
# 또는 tauri = "1.5"        # 더 풍부한 UI 필요시

# 파일 감시
notify = "6.0"

# 설정 파일 경로
directories = "5.0"

# 비동기 런타임 (선택)
tokio = { version = "1", features = ["full"] }

# 로깅
log = "0.4"
env_logger = "0.10"

# Windows 전용
[target.'cfg(windows)'.dependencies]
winreg = "0.52"             # 레지스트리 (URL scheme 등록)

[profile.release]
opt-level = "z"             # 바이너리 크기 최소화
lto = true
strip = true
```

## 🔐 인증 방식: 브라우저 OAuth (Custom URL Scheme)

### 흐름
```
1. 에이전트 → 브라우저로 https://mdflare.com/auth/agent 열기
2. 사용자 → Google 로그인 + "에이전트 연결 승인" 클릭
3. 웹 → mdflare://callback?uid=xxx&username=xxx&token=xxx 로 리다이렉트
4. 에이전트 → URL scheme 수신 → 토큰 저장 → 동기화 시작
```

### Windows URL Scheme 등록 (Rust)

```rust
use winreg::enums::*;
use winreg::RegKey;

fn register_url_scheme(exe_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey("Software\\Classes\\mdflare")?;
    
    key.set_value("", &"URL:MDFlare Protocol")?;
    key.set_value("URL Protocol", &"")?;
    
    let (cmd_key, _) = key.create_subkey("shell\\open\\command")?;
    cmd_key.set_value("", &format!("\"{}\" \"%1\"", exe_path))?;
    
    Ok(())
}
```

### URL Scheme 콜백 수신
앱 시작 시 커맨드라인 인자 확인:
```rust
fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    // mdflare://callback?uid=xxx&username=xxx&token=xxx
    if args.len() > 1 && args[1].starts_with("mdflare://") {
        handle_oauth_callback(&args[1]);
        return;
    }
    
    // 일반 실행
    run_tray_app();
}

fn handle_oauth_callback(url: &str) {
    let url = url::Url::parse(url).unwrap();
    let params: HashMap<_, _> = url.query_pairs().collect();
    
    let username = params.get("username").unwrap();
    let token = params.get("token").unwrap();
    
    // 설정 저장 후 메인 앱으로 전환
    save_config(username, token);
    run_tray_app();
}
```

## 📡 API 명세

### 인증 헤더
```
Authorization: Bearer {token}
```
- GET 요청은 인증 불필요 (공개 읽기)
- PUT/POST/DELETE는 인증 필수

### Rust API Client 구조

```rust
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

struct ApiClient {
    client: Client,
    base_url: String,
    username: String,
    token: String,
}

#[derive(Deserialize)]
struct FileItem {
    name: String,
    path: String,
    #[serde(rename = "type")]
    file_type: String,
    size: Option<u64>,
    modified: Option<String>,
    children: Option<Vec<FileItem>>,
}

#[derive(Deserialize)]
struct FilesResponse {
    user: String,
    files: Vec<FileItem>,
}

#[derive(Deserialize)]
struct FileContent {
    path: String,
    content: String,
    size: u64,
    modified: String,
}

impl ApiClient {
    fn new(base_url: &str, username: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            username: username.to_string(),
            token: token.to_string(),
        }
    }
    
    // 파일 목록 조회
    fn list_files(&self) -> Result<Vec<FileItem>, reqwest::Error> {
        let url = format!("{}/api/{}/files", self.base_url, self.username);
        let resp: FilesResponse = self.client.get(&url).send()?.json()?;
        Ok(resp.files)
    }
    
    // 파일 내용 조회
    fn get_file(&self, path: &str) -> Result<FileContent, reqwest::Error> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client.get(&url).send()?.json()
    }
    
    // 파일 저장
    fn put_file(&self, path: &str, content: &str) -> Result<(), reqwest::Error> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&serde_json::json!({ "content": content }))
            .send()?;
        Ok(())
    }
    
    // 파일 삭제
    fn delete_file(&self, path: &str) -> Result<(), reqwest::Error> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?;
        Ok(())
    }
}
```

## 💾 설정 파일

### 경로
```rust
use directories::ProjectDirs;

fn config_path() -> PathBuf {
    let proj = ProjectDirs::from("com", "mdflare", "agent").unwrap();
    proj.config_dir().join("config.json")
}
// Windows: C:\Users\{User}\AppData\Roaming\mdflare\agent\config.json
```

### 구조
```rust
#[derive(Serialize, Deserialize)]
struct Config {
    api_base: String,       // "https://mdflare.com"
    username: String,       // "user123"
    local_path: String,     // "C:\\Users\\...\\MDFlare"
    api_token: String,      // "agent_abc123..."
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
```

## 🔄 파일 감시 (notify)

```rust
use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc::channel;
use std::time::Duration;

fn watch_files(path: &str, on_change: impl Fn(&Path)) {
    let (tx, rx) = channel();
    
    let mut watcher = watcher(tx, Duration::from_secs(1)).unwrap();
    watcher.watch(path, RecursiveMode::Recursive).unwrap();
    
    loop {
        match rx.recv() {
            Ok(event) => {
                if let notify::DebouncedEvent::Write(path) = event {
                    if path.extension().map_or(false, |e| e == "md") {
                        on_change(&path);
                    }
                }
            }
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
}
```

## 🖥️ 시스템 트레이 (tray-item)

```rust
use tray_item::TrayItem;

fn run_tray_app() {
    let mut tray = TrayItem::new("MDFlare", "flame-icon").unwrap();
    
    tray.add_label("👤 username").unwrap();
    tray.add_label("📁 ~/Documents/MDFlare").unwrap();
    
    tray.inner_mut().add_separator().unwrap();
    
    tray.add_menu_item("🔄 지금 동기화", || {
        sync_now();
    }).unwrap();
    
    tray.add_menu_item("📂 폴더 열기", || {
        open::that(&config.local_path).unwrap();
    }).unwrap();
    
    tray.add_menu_item("🌐 웹에서 열기", || {
        open::that(format!("https://mdflare.com/{}", config.username)).unwrap();
    }).unwrap();
    
    tray.inner_mut().add_separator().unwrap();
    
    tray.add_menu_item("종료", || {
        std::process::exit(0);
    }).unwrap();
    
    // 메시지 루프
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
```

## 🔄 동기화 엔진

```rust
struct SyncEngine {
    api: ApiClient,
    local_path: PathBuf,
    local_hashes: HashMap<String, String>,
}

impl SyncEngine {
    fn full_sync(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 서버 파일 목록
        let remote_files = self.api.list_files()?;
        let remote_paths = self.flatten_files(&remote_files);
        
        // 2. 로컬 파일 목록
        let local_paths = self.scan_local_md_files();
        
        // 3. 서버 → 로컬 (다운로드)
        for path in &remote_paths {
            let local_file = self.local_path.join(path);
            if !local_file.exists() {
                let content = self.api.get_file(path)?;
                std::fs::create_dir_all(local_file.parent().unwrap())?;
                std::fs::write(&local_file, &content.content)?;
                println!("⬇️ {}", path);
            }
        }
        
        // 4. 로컬 → 서버 (업로드)
        for path in &local_paths {
            if !remote_paths.contains(path) {
                let content = std::fs::read_to_string(self.local_path.join(path))?;
                self.api.put_file(path, &content)?;
                println!("⬆️ {}", path);
            }
        }
        
        Ok(())
    }
    
    fn simple_hash(s: &str) -> String {
        let mut hash: i32 = 0;
        for c in s.chars() {
            hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(c as i32);
        }
        format!("{:x}", hash)
    }
}
```

## 📋 구현 체크리스트

- [ ] Cargo 프로젝트 초기화
- [ ] Config 구조체 + 읽기/쓰기
- [ ] `mdflare://` URL scheme 레지스트리 등록
- [ ] 커맨드라인에서 OAuth 콜백 파싱
- [ ] 브라우저 열기 (`open` crate)
- [ ] API 클라이언트 (reqwest)
- [ ] 시스템 트레이 (tray-item)
- [ ] 파일 감시 (notify)
- [ ] 동기화 엔진
- [ ] 30초 주기 풀 동기화 (스레드/타이머)
- [ ] 에러 핸들링
- [ ] 로깅

## 🚀 빌드 & 배포

### 빌드
```bash
# Windows에서
cargo build --release

# 크로스 컴파일 (macOS/Linux에서 Windows 빌드)
cargo build --release --target x86_64-pc-windows-gnu
```

### 결과물
`target/release/mdflare-agent.exe` (~3-5MB)

### 배포
Firebase Storage에 업로드:
- 버킷: `markdownflare.firebasestorage.app`
- 경로: `downloads/win/MDFlare-Agent-{version}-win.zip`

## 📎 참고 코드

macOS Swift 에이전트 (로직 동일):
`~/work/web/mdflare/agent/MDFlareAgent/Sources/main.swift`

---

*작성: 2026-02-06*
*Rust Edition 2021 기준*
