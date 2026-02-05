# MDFlare Windows Agent - 작업 인수인계

> Rust 에이전트 코드 완성됨. Windows에서 빌드 + 테스트만 하면 됨.

## 📁 프로젝트 위치

```
GitHub: https://github.com/ssk-play/mdflare
Rust 에이전트: agent-rust/
```

## ✅ 이미 완료된 것

- [x] Rust 프로젝트 구조 (`Cargo.toml`, `src/main.rs`)
- [x] 시스템 트레이 UI (tray-icon + muda)
- [x] API 클라이언트 (reqwest)
- [x] 파일 감시 (notify)
- [x] 양방향 동기화 로직
- [x] 브라우저 OAuth 로그인 (`mdflare://` URL scheme)
- [x] macOS 빌드 성공 (2.1MB)

## 🎯 Windows에서 할 일

### 1. Rust 설치
```powershell
# PowerShell에서
winget install Rustlang.Rustup
# 또는 https://rustup.rs 에서 다운로드
```

### 2. 프로젝트 클론
```powershell
git clone https://github.com/ssk-play/mdflare.git
cd mdflare/agent-rust
```

### 3. 빌드
```powershell
cargo build --release
```

결과물: `target/release/mdflare-agent.exe`

### 4. 테스트
```powershell
# 실행
.\target\release\mdflare-agent.exe

# 설정 없으면 브라우저 열림 → 로그인 → 자동 설정
```

### 5. URL Scheme 테스트
브라우저에서 로그인 후 `mdflare://callback?...` 이 앱으로 잘 전달되는지 확인.

안 되면 레지스트리 수동 등록:
```powershell
# 관리자 PowerShell
$exePath = "C:\path\to\mdflare-agent.exe"
New-Item -Path "HKCU:\Software\Classes\mdflare" -Force
Set-ItemProperty -Path "HKCU:\Software\Classes\mdflare" -Name "(Default)" -Value "URL:MDFlare Protocol"
New-ItemProperty -Path "HKCU:\Software\Classes\mdflare" -Name "URL Protocol" -Value ""
New-Item -Path "HKCU:\Software\Classes\mdflare\shell\open\command" -Force
Set-ItemProperty -Path "HKCU:\Software\Classes\mdflare\shell\open\command" -Name "(Default)" -Value "`"$exePath`" `"%1`""
```

### 6. 동기화 테스트
1. 로그인 완료 후 폴더 선택
2. 해당 폴더에 `.md` 파일 생성
3. https://mdflare.com/{username} 에서 파일 나타나는지 확인
4. 웹에서 파일 수정 → 로컬에 반영되는지 확인

### 7. 문제 있으면

코드 수정 후:
```powershell
cargo build --release
git add -A
git commit -m "fix: ..."
git push
```

## 📦 배포 준비

빌드 성공하면:

```powershell
# zip 만들기
Compress-Archive -Path "target\release\mdflare-agent.exe" -DestinationPath "MDFlare-Agent-1.0.0-win.zip"
```

그 다음 Firebase Storage에 업로드하거나, 나한테 zip 파일 보내줘.

## 🔧 코드 구조 (참고)

```
agent-rust/
├── Cargo.toml          # 의존성
└── src/
    └── main.rs         # 전체 코드 (~500줄)
        ├── Config          # 설정 파일 관리
        ├── ApiClient       # REST API 호출
        ├── SyncEngine      # 동기화 로직
        └── run_tray_app()  # 시스템 트레이 UI
```

### 설정 파일 위치
- Windows: `%APPDATA%\mdflare\agent\config.json`
- macOS: `~/Library/Application Support/com.mdflare.agent/config.json`

### 주요 의존성
- `reqwest` - HTTP 클라이언트
- `notify` - 파일 시스템 감시
- `tray-icon` + `muda` - 시스템 트레이
- `tao` - 이벤트 루프
- `winreg` (Windows만) - 레지스트리

## 🌐 서비스 정보

- **웹:** https://mdflare.com
- **API:** https://mdflare.com/api/{username}/...
- **로그인 페이지:** https://mdflare.com/auth/agent

## ❓ 질문

macOS에서 테스트 완료됨. Windows 특이사항만 확인하면 됨.
문제 생기면 GitHub Issue 또는 직접 연락!

---

*작성: 2026-02-06*
*Rust 1.93.0 / macOS 빌드 완료*
