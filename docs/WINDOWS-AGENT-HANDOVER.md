# MDFlare Windows Agent 개발 인수인계 문서

> 이 문서 하나로 Windows 에이전트 개발을 바로 시작할 수 있습니다.

## 📁 프로젝트 구조

```
~/work/web/mdflare/
├── web/                    # 웹 프론트엔드 + API (Cloudflare Pages)
│   ├── src/                # React 앱
│   ├── functions/          # Cloudflare Pages Functions (API)
│   └── dist/               # 빌드 결과물
├── agent/                  # macOS 에이전트 (참고용)
│   └── MDFlareAgent/
│       └── Sources/
│           └── main.swift  # 전체 코드 (단일 파일)
└── docs/                   # 문서
```

## 🌐 서비스 정보

- **웹사이트:** https://mdflare.com
- **API Base:** https://mdflare.com/api
- **GitHub:** https://github.com/ssk-play/mdflare

## 🔐 인증 방식: 브라우저 OAuth (Custom URL Scheme)

### 흐름
```
1. 에이전트 → 브라우저로 https://mdflare.com/auth/agent 열기
2. 사용자 → Google 로그인 + "에이전트 연결 승인" 클릭
3. 웹 → mdflare://callback?uid=xxx&username=xxx&token=xxx 로 리다이렉트
4. 에이전트 → URL scheme 수신 → 토큰 저장 → 동기화 시작
```

### Windows에서 Custom URL Scheme 등록
레지스트리에 등록 필요:
```
HKEY_CURRENT_USER\Software\Classes\mdflare
├── (Default) = "URL:MDFlare Protocol"
├── URL Protocol = ""
└── shell\open\command\
    └── (Default) = "C:\Path\To\MDFlareAgent.exe" "%1"
```

또는 설치 시 자동 등록하는 코드 필요.

## 📡 API 명세

### 인증 헤더
```
Authorization: Bearer {token}
```
- GET 요청은 인증 불필요 (공개 읽기)
- PUT/POST/DELETE는 인증 필수

### 엔드포인트

#### 1. 파일 목록 조회
```
GET /api/{username}/files

Response:
{
  "user": "username",
  "files": [
    {
      "name": "note.md",
      "path": "note.md",
      "type": "file",
      "size": 1234,
      "modified": "2024-02-05T12:00:00.000Z"
    },
    {
      "name": "folder",
      "path": "folder",
      "type": "folder",
      "children": [...]
    }
  ]
}
```

#### 2. 파일 내용 조회
```
GET /api/{username}/file/{path}

Response:
{
  "path": "folder/note.md",
  "content": "# Hello\n\nContent here...",
  "size": 1234,
  "modified": "2024-02-05T12:00:00.000Z"
}
```

#### 3. 파일 저장/생성
```
PUT /api/{username}/file/{path}
Authorization: Bearer {token}
Content-Type: application/json

Body:
{
  "content": "# New content\n\nHello world"
}

Response:
{
  "saved": true,
  "path": "note.md",
  "size": 28
}
```

#### 4. 파일 삭제
```
DELETE /api/{username}/file/{path}
Authorization: Bearer {token}

Response:
{
  "deleted": true,
  "path": "note.md"
}
```

#### 5. 파일/폴더 이름 변경
```
POST /api/{username}/rename
Authorization: Bearer {token}
Content-Type: application/json

Body:
{
  "oldPath": "old-name.md",
  "newPath": "new-name.md"
}
```

## 💾 로컬 설정 파일

macOS: `~/.mdflare/config.json`
Windows 권장: `%APPDATA%\MDFlare\config.json`

```json
{
  "apiBase": "https://mdflare.com",
  "username": "user123",
  "localPath": "C:\\Users\\Username\\Documents\\MDFlare",
  "apiToken": "agent_abc123..."
}
```

## 🔄 동기화 로직

### 기본 원칙
1. **R2가 절대저장소** — 충돌 시 서버 우선 (또는 타임스탬프 비교)
2. **양방향 동기화** — 로컬 변경 → 서버, 서버 변경 → 로컬
3. **마크다운만** — `.md` 파일만 동기화

### 동기화 주기
- **파일 감시:** 로컬 파일 변경 시 즉시 업로드 (1초 debounce)
- **풀 동기화:** 30초마다 전체 파일 목록 비교

### 파일 감시 (Windows)
- `FileSystemWatcher` 클래스 사용 (.NET)
- 또는 `ReadDirectoryChangesW` API (Win32)

### 동기화 흐름
```
1. 서버에서 파일 목록 가져오기
2. 로컬 파일 목록 스캔
3. 서버에만 있는 파일 → 다운로드
4. 로컬에만 있는 파일 → 업로드
5. 양쪽에 있는 파일 → 해시 비교 후 필요시 동기화
```

### 간단한 해시 함수 (내용 비교용)
```javascript
function simpleHash(str) {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return hash.toString(36);
}
```

## 🖥️ UI 요구사항

### 시스템 트레이 앱
- 트레이 아이콘: 🔥 또는 커스텀 아이콘
- 상태 표시: "동기화 중...", "대기 중 · 15개 파일", "오류" 등

### 트레이 메뉴
```
👤 {username}
📁 {동기화 폴더 경로}
─────────────────
🔄 지금 동기화
📂 폴더 열기
🌐 웹에서 열기
─────────────────
⚙️ 설정
종료
```

### 초기 설정 화면 (미설정 시)
```
🔐 브라우저로 로그인    ← 메인 버튼 (브라우저 열기)
⚙️ 수동 설정           ← 토큰 직접 입력 옵션
```

### 로그인 성공 후
```
🎉 로그인 성공!
사용자: {username}

동기화 폴더를 선택하세요.
[폴더 선택] [기본 폴더 사용] [취소]
```

## 🛠️ 기술 스택 권장

### 옵션 1: C# / .NET (권장)
- WPF 또는 WinForms
- `System.IO.FileSystemWatcher`
- `HttpClient`
- 단일 exe 배포 가능

### 옵션 2: Rust + Tauri
- 크로스플랫폼 가능
- 작은 바이너리

### 옵션 3: Electron
- 웹 기술 재사용
- 용량 큼 (비추)

## 📋 구현 체크리스트

- [ ] 시스템 트레이 앱 기본 구조
- [ ] 설정 파일 읽기/쓰기
- [ ] `mdflare://` URL scheme 등록
- [ ] 브라우저 OAuth 로그인 (URL scheme 콜백 수신)
- [ ] API 클라이언트 (GET/PUT/DELETE)
- [ ] 파일 목록 조회 + 파싱
- [ ] 로컬 파일 스캔
- [ ] 파일 다운로드/업로드
- [ ] FileSystemWatcher로 로컬 변경 감지
- [ ] 30초 주기 풀 동기화
- [ ] 에러 핸들링 + 재시도
- [ ] 로그 기록

## 📎 참고: macOS 에이전트 코드

`~/work/web/mdflare/agent/MDFlareAgent/Sources/main.swift` 참고

주요 클래스:
- `ConfigManager` — 설정 파일 관리
- `APIClient` — REST API 호출
- `FileWatcher` — FSEvents 파일 감시
- `SyncEngine` — 동기화 로직
- `AppDelegate` — 메뉴바 UI + URL scheme 핸들링

## 🚀 빌드 & 배포

### 배포 파일
- `MDFlare-Agent-{version}-win.zip`
- 내부: `MDFlare Agent.exe` + 필요한 DLL

### 다운로드 페이지 업데이트
`~/work/web/mdflare/web/src/pages/Download.jsx`에 Windows 다운로드 링크 추가

### 호스팅
Firebase Storage 사용:
- 버킷: `markdownflare.firebasestorage.app`
- 경로: `downloads/win/MDFlare-Agent-{version}-win.zip`

## ❓ 질문 있으면

macOS 에이전트 코드(`main.swift`)를 참고하면 거의 모든 로직이 있음.
API는 웹에서 직접 테스트 가능: https://mdflare.com/{username}

---

*작성: 2026-02-06*
*MDFlare Agent v1.0.3 기준*
