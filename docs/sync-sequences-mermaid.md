# MDFlare 동기화 시퀀스 다이어그램 (Mermaid)

## 1. 최초 가입 — R2 생성

```mermaid
sequenceDiagram
    actor User
    participant Web
    participant Workers
    participant R2

    User->>Web: 가입
    Web->>Workers: POST /signup
    Workers->>R2: 버킷 생성
    R2-->>Workers: 생성 완료
    Workers-->>Web: 완료
    Web-->>User: 대시보드<br/>"로컬 에이전트를 설치하세요"
```

## 2. 로컬 에이전트 최초 연결 — 전체 업로드

```mermaid
sequenceDiagram
    participant Agent as Agent (Mac)
    participant Workers
    participant R2

    Agent->>Workers: 인증 요청
    Workers-->>Agent: 토큰 발급

    Agent->>Workers: GET /files
    Workers->>R2: list
    R2-->>Workers: [] 비어있음
    Workers-->>Agent: 빈 목록

    Note over Agent: 로컬 폴더 스캔: 50개 .md

    loop 각 파일
        Agent->>Workers: PUT file
        Workers->>R2: put
        R2-->>Workers: ✅
    end

    Note over Agent, R2: 최초 동기화 완료 (50개)
```

## 3. 로컬에서 파일 수정

```mermaid
sequenceDiagram
    actor User
    participant Agent as Agent (Mac)
    participant Workers
    participant R2
    participant Web as Web (열려있음)

    User->>Agent: 파일 수정
    Note over Agent: FSEvents 감지
    Agent->>Workers: PUT file
    Workers->>R2: put
    R2-->>Workers: ✅
    Workers-->>Agent: 저장 완료

    Note over Web: 폴링 or 탭 포커스
    Web->>Workers: GET file
    Workers->>R2: get
    R2-->>Workers: 최신 내용
    Workers-->>Web: 최신 내용
    Note over Web: ✅ 화면 갱신
```

## 4. 웹에서 파일 수정

```mermaid
sequenceDiagram
    actor User
    participant Web
    participant Workers
    participant R2
    participant Agent as Agent (Mac)

    User->>Web: 타이핑
    Note over Web: 1초 debounce
    Web->>Workers: PUT file
    Workers->>R2: put
    R2-->>Workers: ✅
    Workers-->>Web: 저장 완료

    Note over Agent: 폴링 (변경 체크)
    Agent->>Workers: GET /files (수정시간 비교)
    Workers->>R2: list
    R2-->>Workers: 변경 있음
    Workers-->>Agent: 변경 목록
    Agent->>Workers: GET file
    Workers-->>Agent: 최신 내용
    Note over Agent: ✅ 로컬 파일 갱신
```

## 5. 로컬 꺼짐 → 웹에서 수정 → 로컬 다시 켜짐

```mermaid
sequenceDiagram
    participant Agent as Agent (Mac)
    participant Web
    participant Workers
    participant R2

    Note over Agent: ❌ PC 종료

    Web->>Workers: PUT fileA
    Workers->>R2: put fileA ✅
    Web->>Workers: PUT fileB
    Workers->>R2: put fileB ✅

    Note over Agent, R2: ⏳ 3시간 경과

    Note over Agent: 🔌 PC 켜짐 (에이전트 자동 시작)
    Agent->>Workers: GET /files
    Workers->>R2: list (전체 목록 + 수정시간)
    R2-->>Workers: 전체 목록
    Workers-->>Agent: 파일 목록

    Note over Agent: 로컬과 비교

    rect rgb(40, 80, 40)
        Note over Agent: fileA: R2가 최신 → 다운로드
        Agent->>Workers: GET fileA
        Workers->>R2: get
        R2-->>Workers: 내용
        Workers-->>Agent: fileA 내용
        Note over Agent: ✅ 로컬 fileA 갱신
    end

    rect rgb(40, 80, 40)
        Note over Agent: fileB: R2가 최신 → 다운로드
        Agent->>Workers: GET fileB
        Workers-->>Agent: fileB 내용
        Note over Agent: ✅ 로컬 fileB 갱신
    end
```

## 6. 충돌 — 양쪽 동시 수정

```mermaid
sequenceDiagram
    participant Agent as Agent (Mac)
    participant Web
    participant Workers
    participant R2
    participant Orphan as 🏥 고아원

    Note over Agent: ❌ PC 종료

    Web->>Workers: PUT readme (v2-web)
    Workers->>R2: put v2-web ✅

    Note over Agent, R2: PC 꺼진 동안 로컬에서도 수정됨

    Note over Agent: 🔌 PC 켜짐 (readme = v2-local)
    Agent->>Workers: PUT readme (v2-local)

    Note over Workers: ⚠️ 충돌 감지!<br/>R2: v2-web<br/>요청: v2-local

    Note over Workers: 우선순위 정책:<br/>최신 타임스탬프

    alt v2-web이 최신
        Workers->>Orphan: v2-local 보관 (30일)
        Workers-->>Agent: 충돌 알림<br/>"R2 버전 유지, 로컬은 고아원"
        Agent->>Workers: GET readme
        Workers->>R2: get
        R2-->>Workers: v2-web
        Workers-->>Agent: v2-web
        Note over Agent: ✅ 로컬 = v2-web
    else v2-local이 최신
        Workers->>Orphan: v2-web 보관 (30일)
        Workers->>R2: put v2-local ✅
        Workers-->>Agent: 동기화 완료
    end
```

## 7. 멀티 로컬 — Mac 2대

```mermaid
sequenceDiagram
    participant A as Agent-A (집)
    participant B as Agent-B (사무실)
    participant Workers
    participant R2

    A->>Workers: PUT file (수정)
    Workers->>R2: put ✅

    Note over B: 폴링 (변경 체크)
    B->>Workers: GET /files
    Workers->>R2: list
    R2-->>Workers: 변경 있음
    Workers-->>B: 변경 목록

    B->>Workers: GET file
    Workers->>R2: get
    R2-->>Workers: 최신 내용
    Workers-->>B: 최신 내용
    Note over B: ✅ 로컬 갱신

    Note over A, B: 양쪽 Mac이 항상 R2와 동일
```

## 8. 멀티 웹 — 브라우저 2개

```mermaid
sequenceDiagram
    participant A as Web-A
    participant B as Web-B
    participant Workers
    participant R2

    A->>Workers: PUT file (수정)
    Workers->>R2: put ✅
    Workers-->>A: 저장 완료

    Note over B: 탭 포커스

    B->>Workers: GET file
    Workers->>R2: get
    R2-->>Workers: 최신 내용
    Workers-->>B: 최신 내용
    Note over B: ✅ 화면 갱신
```
