# 🔥 MDFlare

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![GitHub stars](https://img.shields.io/github/stars/ssk-play/mdflare)](https://github.com/ssk-play/mdflare/stargazers)

**내 마크다운 폴더를 웹에서 열다.**

로컬 마크다운 폴더를 클라우드와 실시간 동기화하고, 어디서든 브라우저로 편집.

🌐 **https://mdflare.com**

---

## 왜 MDFlare인가?

### 문제: Obsidian의 한계

[Obsidian](https://obsidian.md)은 훌륭한 마크다운 에디터다. 로컬 파일 기반이라 내 데이터를 완전히 소유할 수 있고, 마크다운이라 AI와 궁합이 환상적이다.

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
- ✅ **셀프 호스팅 가능** — Cloudflare 무료 티어로 운영

---

## 주요 기능

- ✏️ **마크다운 에디터** — CodeMirror 6, 자동 저장
- 📂 **파일/폴더 관리** — 생성, 이름 변경, 삭제, 이동
- 🖱️ **드래그 & 드롭** — 파일을 폴더로 끌어서 이동
- 👁️ **미리보기** — Edit / Split / Preview 3모드
- 🔄 **실시간 동기화** — 다중 클라이언트 지원
- 🖥️ **macOS 에이전트** — 메뉴바 앱, 로컬 폴더 양방향 동기화
- 📱 **모바일 반응형** — 터치 최적화
- 🔐 **Google 로그인** — Firebase Authentication

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

**동기화 전략:** 에이전트가 R2에 파일을 저장하면 Firebase Realtime DB에 메타데이터(해시, 수정시간)를 기록하고, 웹 클라이언트는 RTDB를 실시간 구독하여 변경을 즉시 감지합니다. 상세 시퀀스는 [동기화 시퀀스 다이어그램](docs/sync-sequences.md) 참고.

---

## 시작하기

### 필수 조건

- Node.js 18+
- Cloudflare 계정 (무료)
- Firebase 프로젝트 (무료)

### 설치

```bash
git clone https://github.com/ssk-play/mdflare.git
cd mdflare
npm install
```

### 환경 설정

`.env` 파일 생성:

```bash
CLOUDFLARE_API_TOKEN=your_cloudflare_api_token
```

`web/src/firebase.js`에서 Firebase 설정 수정.

### 로컬 개발

```bash
# 전체 실행
npm run dev

# 또는 개별 실행
cd server && node index.js  # API 서버 :3001
cd web && npm run dev       # 프론트엔드 :5173
```

### 배포

```bash
cd web
npm run build
npx wrangler pages deploy dist --project-name=your-project-name
```

---

## 프로젝트 구조

```
mdflare/
├── web/                    # 웹 프론트엔드 + API
│   ├── src/                # React 앱
│   ├── functions/          # Cloudflare Pages Functions
│   └── dist/               # 빌드 결과물
├── server/                 # 로컬 개발 서버
└── agent/                  # 크로스플랫폼 동기화 에이전트 (Rust)
```

---

## 기여하기

기여를 환영합니다! 

1. 이 저장소를 Fork
2. 기능 브랜치 생성 (`git checkout -b feature/amazing-feature`)
3. 변경사항 커밋 (`git commit -m 'Add amazing feature'`)
4. 브랜치에 Push (`git push origin feature/amazing-feature`)
5. Pull Request 생성

### 이슈 제출

버그 리포트나 기능 제안은 [GitHub Issues](https://github.com/ssk-play/mdflare/issues)에서 해주세요.

---

## 로드맵

- [ ] Windows/Linux 에이전트 (Rust)
- [ ] 버전 히스토리
- [ ] 공동 편집
- [ ] 이미지 업로드
- [ ] 플러그인 시스템

---

## 라이선스

이 프로젝트는 [AGPL-3.0 라이선스](LICENSE) 하에 배포됩니다.

**AGPL-3.0을 선택한 이유:**
- 오픈소스로 자유롭게 사용, 수정, 배포 가능
- 수정된 버전을 서비스로 제공할 경우 소스 공개 필요
- 커뮤니티 기여를 장려하면서 상업적 폐쇄적 사용 방지

---

## 감사의 말

- [Obsidian](https://obsidian.md) — 영감을 준 최고의 마크다운 에디터
- [Cloudflare](https://cloudflare.com) — 관대한 무료 티어
- [Firebase](https://firebase.google.com) — 간편한 인증과 실시간 DB

---

Made with ❤️ by [SSK](https://github.com/ssk-play)
