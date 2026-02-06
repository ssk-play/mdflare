# 🔥 MDFlare

**내 마크다운 폴더를 웹에서 열다.**

로컬 마크다운 폴더를 클라우드와 실시간 동기화하고, 어디서든 브라우저로 편집.

🌐 **https://mdflare.com**

---

## 왜 MDFlare인가?

### 문제: Obsidian의 한계

[Obsidian](https://obsidian.md)은 훌륭한 마크다운 에디터다. 로컬 파일 기반이라 내 데이터를 완전히 소유할 수 있고, 마크다운이라 AI와 궁합이 환상적이다. Claude나 ChatGPT에 마크다운 파일을 던지면 바로 이해하고, 수정해서 돌려준다. Notion의 독자 포맷과는 차원이 다르다.

**하지만 치명적인 단점이 있다 — 온라인이 안 된다.**

- 회사 PC에서 작성한 메모를 카페에서 이어 쓰려면?
- 핸드폰으로 잠깐 확인하고 싶은데?
- Obsidian Sync는 **$8/월**. 그것도 동기화만 되지, 웹 에디터는 없다.

### 대안들의 한계

| 서비스 | 문제점 |
|--------|--------|
| **Notion** | 독자 포맷 → AI 연동 불편, 데이터 종속 |
| **Obsidian Sync** | $8/월, 웹 에디터 없음 |
| **Obsidian Publish** | $16/월, 읽기 전용 |
| **Git 동기화** | 충돌 지옥, 비개발자 진입장벽 |
| **iCloud/Dropbox** | 동기화 느림, 충돌 해결 수동 |

### 해결: MDFlare

**"로컬 마크다운의 장점 + 웹 접근성"**

```
Obsidian (로컬)  ←──동기화──→  MDFlare (웹)
     │                              │
     └──── 같은 .md 파일들 ─────────┘
```

- ✅ **내 마크다운 폴더 그대로** — Obsidian vault를 MDFlare에 연결
- ✅ **어디서든 웹으로** — 브라우저만 있으면 편집
- ✅ **AI 친화적** — 순수 마크다운, 복붙 한 방
- ✅ **거의 무료** — 도메인 $10.44/년, 나머지 무료 티어

Notion처럼 편하고, Obsidian처럼 자유롭다.

---

## 아키텍처

```
┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
│  브라우저     │────▶│  Cloudflare Pages │────▶│  Cloudflare  │
│  (React)     │◀────│  + Workers API    │◀────│  R2 Storage  │
└─────────────┘     └──────────────────┘     └─────────────┘
                           │                        ▲
                    Firebase│Realtime DB             │ 동기화
                     (변경 감지)                      │
                           │                  ┌─────────────┐
                           └─────────────────▶│  macOS Agent │
                                              │  (Swift)     │
                                              └──────┬──────┘
                                                     │
                                              ┌──────▼──────┐
                                              │  로컬 폴더   │
                                              │  ~/notes/    │
                                              └─────────────┘
```

---

## 서비스 구성

| 기능 | 서비스 | 용도 | 비용 |
|------|--------|------|------|
| **웹 호스팅** | Cloudflare Pages | React 프론트엔드 배포 | 무료 |
| **API** | Cloudflare Workers (Pages Functions) | REST API 엔드포인트 | 무료 (100K req/일) |
| **파일 저장** | Cloudflare R2 | 마크다운 파일 + 유저 데이터 저장 | 무료 (10GB) |
| **도메인** | Cloudflare DNS | mdflare.com | $10.44/년 |
| **로그인** | Firebase Authentication | Google 소셜 로그인 | 무료 |
| **실시간 동기화** | Firebase Realtime Database | 파일 변경 감지 리스너 | 무료 (100 동시연결) |
| **앱 다운로드** | Firebase Storage (US) | macOS 에이전트 zip 호스팅 | 무료 (5GB) |
| **소스 관리** | GitHub | 코드 저장소 | 무료 |

### 비용 요약
- **초기:** 도메인 $10.44/년만. 나머지 전부 무료
- **유저 1,000명:** $0/월
- **유저 10,000명:** R2 $1.35/월

---

## 주요 기능

- ✏️ **마크다운 에디터** — CodeMirror 6, 자동 저장 (1초 debounce)
- 📂 **파일/폴더 관리** — 생성, 이름 변경, 삭제, 복제
- 🖱️ **드래그 & 드롭** — 파일을 폴더로 끌어서 이동
- ⋮ **컨텍스트 메뉴** — 데스크탑 우클릭 + 모바일 ⋮ 버튼
- 📁 **폴더 포커스** — 폴더 선택 후 그 안에 파일 생성
- 👁️ **미리보기** — Edit / Split / Preview 3모드
- 📋 **클립보드 복사** — 원클릭 전체 내용 복사
- 🔄 **실시간 동기화** — Firebase Realtime DB로 다중 클라이언트 감지
- 🖥️ **macOS 에이전트** — 메뉴바 앱, 로컬 폴더 양방향 동기화
- 📱 **모바일 반응형** — 접이식 사이드바, 터치 최적화
- 🔐 **Google 로그인** — Firebase Authentication
- 🎲 **샘플 생성** — 테스트용 파일/폴더 원클릭 생성
- 🆓 **거의 무료** — 도메인 $10.44/년, 나머지 전부 무료 티어

---

## 기능별 상세

### 🔐 로그인 — Firebase Authentication
- Google OAuth 소셜 로그인
- 첫 로그인 시 username 설정 (`/setup`)
- 웹 클라이언트: Firebase SDK (`signInWithPopup`)
- 에이전트: API 토큰 인증 (`Authorization: Bearer {token}`)

### 📁 파일 저장 — Cloudflare R2
- 마크다운 파일 원본 저장 (절대저장소)
- 유저 데이터: `_users/`, `_usernames/`, `_tokens/`
- R2 키 구조: `{uid}/{파일경로}`
- 전송료 무료 (egress free)

### 🔄 실시간 동기화 — Firebase Realtime Database
- 파일 저장 시 메타데이터(해시, 크기, 시간) 기록
- 다른 클라이언트가 리스너로 변경 감지
- DB 구조: `mdflare/{userId}/files/{safeKey}`
- 무료 100 동시연결 → ~2,000명까지 커버

### 📥 앱 다운로드 — Firebase Storage
- macOS 에이전트 zip 파일 호스팅
- US 리전 (글로벌 접근 최적화)
- 경로: `downloads/mac/MDFlare-Agent-{version}-mac.zip`
- 다운로드 페이지: `mdflare.com/download`

---

## 프로젝트 구조

```
mdflare/
├── web/                    # 웹 프론트엔드 + API
│   ├── src/
│   │   ├── App.jsx         # 라우터
│   │   ├── firebase.js     # Firebase 설정
│   │   ├── pages/
│   │   │   ├── Landing.jsx     # 랜딩 페이지
│   │   │   ├── SetUsername.jsx # username 설정
│   │   │   ├── Workspace.jsx   # 에디터 (CodeMirror 6)
│   │   │   └── Download.jsx    # 다운로드 페이지
│   │   └── style.css
│   ├── functions/          # Cloudflare Pages Functions (API)
│   │   └── api/
│   │       ├── [userId]/
│   │       │   ├── _middleware.js  # username→uid 리졸브 + 인증
│   │       │   ├── files.js        # GET 파일 트리
│   │       │   ├── file/[[path]].js # GET/PUT/DELETE 파일
│   │       │   └── rename.js       # POST 이름 변경
│   │       ├── username/
│   │       │   ├── check.js    # GET 중복 체크
│   │       │   ├── register.js # POST 등록
│   │       │   └── resolve.js  # GET uid↔username 조회
│   │       └── token/
│   │           └── generate.js # POST API 토큰 발급
│   └── dist/               # 빌드 결과물
├── server/                 # 로컬 개발 서버 (Express)
│   └── index.js
├── agent/                  # macOS 동기화 에이전트
│   ├── MDFlareAgent/
│   │   ├── Sources/
│   │   │   ├── main.swift  # 전체 코드 (AppKit 메뉴바 앱)
│   │   │   └── Info.plist
│   │   └── project.yml     # xcodegen 설정
│   ├── build/              # 빌드 결과물 (.app, .zip)
│   └── deploy.sh           # 배포 스크립트 (버전업+빌드+업로드)
└── .env                    # Cloudflare API Token
```

---

## 인증 흐름

### 웹 클라이언트
```
Google 로그인 → Firebase Auth → uid 획득
→ 쓰기 요청 시 X-Firebase-UID 헤더 전송
→ 미들웨어에서 uid 검증
```

### macOS 에이전트
```
웹에서 🔑 API 토큰 발급 → 에이전트에 입력
→ 쓰기 요청 시 Authorization: Bearer {token} 전송
→ 미들웨어에서 _tokens/{token} 조회 → uid 검증
```

### 권한
- **GET (읽기):** 공개 — 누구나 마크다운 열람 가능
- **PUT/POST/DELETE (쓰기):** 인증 필수 — 본인만 수정 가능

---

## 개발

### 로컬 서버
```bash
cd server && npm install && node index.js
# 🔥 http://localhost:3001
```

### 웹 프론트엔드
```bash
cd web && npm install && npm run dev
# ⚡ http://localhost:5173
```

### 배포
```bash
# 웹 배포
cd web && npm run build
CLOUDFLARE_API_TOKEN=xxx npx wrangler pages deploy dist --project-name=mdflare

# 에이전트 배포 (패치 버전 자동 증가)
cd agent && ./deploy.sh
```

---

## 설정

### Cloudflare
- **Account ID:** 271486b4840b7f2c5af74ed0f11b87d0
- **R2 버킷:** mdflare-vault
- **Pages 프로젝트:** mdflare
- **도메인:** mdflare.com → mdflare.pages.dev

### Firebase (markdownflare)
- **프로젝트:** markdownflare
- **Auth:** Google 로그인
- **Realtime DB:** markdownflare-default-rtdb
- **Storage:** markdownflare.firebasestorage.app (US)

---

## 수익 모델

| 플랜 | 가격 | 용량 | 기능 |
|------|------|------|------|
| Free | $0 | 10MB | 기본 편집 |
| Pro | $5/월 | 1GB | 버전 히스토리 |
| Team | $12/월/인 | 무제한 | 공동 편집 |

---

## 라이선스

MIT © 2026 MDFlare
