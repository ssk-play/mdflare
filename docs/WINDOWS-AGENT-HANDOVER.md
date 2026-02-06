# MDFlare Agent 작업 인수인계

> ⚠️ 모든 에이전트 코드 작성 + 빌드 완료됨. **실제 동작 테스트는 아직 안 함.**

## 📋 현재 상태

| 구분 | 상태 | 비고 |
|------|------|------|
| macOS Swift 에이전트 | ✅ 코드 완성, 빌드 성공 | 실행 테스트 필요 |
| Rust 에이전트 | ✅ 코드 완성, macOS 빌드 성공 | Windows 빌드 + 테스트 필요 |
| 웹 OAuth 플로우 | ✅ 구현 완료 | `/auth/agent` 페이지 |
| URL Scheme | ✅ 코드에 구현됨 | 실제 등록/호출 테스트 필요 |

## 📁 프로젝트 위치

```
GitHub: https://github.com/ssk-play/mdflare
├── agent/          # 크로스플랫폼 Rust 에이전트
├── web/            # 웹 프론트엔드 (React)
└── docs/           # 문서
```

---

## 🍎 macOS 에이전트 (Swift) - 테스트 필요

### 다운로드
https://mdflare.com/download 에서 zip 다운로드

### 테스트 순서
1. zip 풀고 `MDFlareAgent.app` 실행
2. 메뉴바에 🔥 아이콘 나타나는지 확인
3. "브라우저로 로그인" 클릭
4. 브라우저에서 Google 로그인 → "에이전트 연결 승인" 클릭
5. `mdflare://callback?...` URL이 에이전트로 전달되는지 확인
6. 폴더 선택 다이얼로그 나타나는지 확인
7. 동기화 시작되는지 확인

### 예상 문제점
- URL scheme (`mdflare://`) 등록 안 될 수 있음
- 앱 공증(notarization) 없어서 "알 수 없는 개발자" 경고
- 해결: 시스템설정 > 개인정보 보호 및 보안 > "확인 없이 열기"

### 설정 파일 위치
```
~/.mdflare/config.json
```

---

## 🦀 Windows Rust 에이전트 - 빌드 + 테스트 필요

### 1. Rust 설치
```powershell
winget install Rustlang.Rustup
# 또는 https://rustup.rs
```

### 2. 프로젝트 클론
```powershell
git clone https://github.com/ssk-play/mdflare.git
cd mdflare/agent
```

### 3. 빌드
```powershell
cargo build --release
```
결과물: `target\release\mdflare-agent.exe`

### 4. 실행 테스트
```powershell
.\target\release\mdflare-agent.exe
```
- 설정 없으면 자동으로 브라우저 열림
- 로그인 후 `mdflare://callback?...`이 앱으로 전달되어야 함

### 5. URL Scheme 확인

앱 실행 시 자동으로 레지스트리에 등록되도록 코드에 구현됨.
안 되면 수동 등록:

```powershell
# 관리자 PowerShell
$exePath = (Resolve-Path ".\target\release\mdflare-agent.exe").Path
New-Item -Path "HKCU:\Software\Classes\mdflare" -Force
Set-ItemProperty -Path "HKCU:\Software\Classes\mdflare" -Name "(Default)" -Value "URL:MDFlare Protocol"
New-ItemProperty -Path "HKCU:\Software\Classes\mdflare" -Name "URL Protocol" -Value ""
New-Item -Path "HKCU:\Software\Classes\mdflare\shell\open\command" -Force
Set-ItemProperty -Path "HKCU:\Software\Classes\mdflare\shell\open\command" -Name "(Default)" -Value "`"$exePath`" `"%1`""
```

### 6. 동기화 테스트
1. 로그인 완료 → 기본 폴더 `내 문서/MDFlare` 생성됨
2. 해당 폴더에 `test.md` 파일 만들기
3. https://mdflare.com/{username} 에서 파일 보이는지 확인
4. 웹에서 수정 → 로컬에 반영되는지 확인 (30초 주기)

### 설정 파일 위치
```
%APPDATA%\mdflare\agent\config.json
```

---

## 🔐 OAuth 인증 플로우

```
[에이전트] 
    │ 설정 없음 → 브라우저 열기
    ▼
[브라우저] https://mdflare.com/auth/agent
    │ Google 로그인 → "에이전트 연결 승인" 클릭
    ▼
[서버] /api/token/agent
    │ 새 토큰 발급
    ▼
[브라우저] → mdflare://callback?username=xxx&token=xxx
    │
    ▼
[에이전트] URL scheme 핸들러가 받음
    │ config.json에 저장 → 동기화 시작
    ▼
[완료] 🎉
```

---

## 🐛 트러블슈팅

### URL Scheme이 작동 안 함
- **Windows:** 레지스트리 수동 등록 (위 참고)
- **macOS:** `LSSetDefaultHandlerForURLScheme` 호출 필요 (코드에 있음)
- 브라우저 종류에 따라 `mdflare://` 차단될 수 있음

### 빌드 에러 (Windows)
```powershell
# Visual Studio Build Tools 필요할 수 있음
winget install Microsoft.VisualStudio.2022.BuildTools
```

### 동기화 안 됨
- 콘솔 출력 확인 (에러 메시지)
- 네트워크 확인 (https://mdflare.com 접속)
- API 토큰 만료 확인

---

## 📦 배포 (빌드 성공 후)

### Windows
```powershell
Compress-Archive -Path "target\release\mdflare-agent.exe" -DestinationPath "MDFlare-Agent-win-x64.zip"
```

### 업로드
Firebase Storage 또는 GitHub Releases에 업로드

---

## 🔧 코드 구조

### Rust 에이전트 (`agent/src/main.rs`)
```
~500줄, 단일 파일
├── Config              # 설정 로드/저장
├── ApiClient           # REST API (reqwest)
├── SyncEngine          # 동기화 로직
├── parse_oauth_callback() # URL scheme 파싱
├── register_url_scheme()  # Windows 레지스트리
└── run_tray_app()      # 시스템 트레이 (tray-icon + muda)
```

### 주요 의존성 (Cargo.toml)
- `reqwest` - HTTP
- `notify` + `notify-debouncer-mini` - 파일 감시
- `tray-icon` + `muda` - 시스템 트레이
- `tao` - 이벤트 루프
- `winreg` - Windows 레지스트리
- `walkdir` - 디렉토리 순회

---

## ❓ 확인 필요한 것들

1. **URL Scheme:** 브라우저 → 에이전트 호출이 실제로 되는지
2. **파일 감시:** notify가 Windows에서 잘 작동하는지
3. **시스템 트레이:** Windows 11에서 아이콘 제대로 뜨는지
4. **한글 경로:** 폴더/파일명에 한글 있을 때 문제없는지
5. **폴더 선택:** Rust 에이전트는 현재 기본 폴더 자동 설정 (선택 UI 없음)

---

## 🌐 관련 링크

- **서비스:** https://mdflare.com
- **로그인 페이지:** https://mdflare.com/auth/agent
- **다운로드:** https://mdflare.com/download
- **GitHub:** https://github.com/ssk-play/mdflare

---

*최종 업데이트: 2026-02-06 08:20*
*상태: 코드 완성, 실제 테스트 대기*
