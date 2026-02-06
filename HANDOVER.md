# MDFlare Handover - 2026-02-06

## 🎯 현재 상태

### Private Vault 터널링 변경 작업 진행 중

**문제:**
- `bore.pub` 서버가 다운됨 (포트 2200 연결 거부)
- 회사 방화벽 아님 - LTE에서도 bore.pub 접속 불가 확인됨

**시도한 대안들:**

| 서비스 | 결과 | 비고 |
|--------|------|------|
| bore.pub | ❌ 서버 다운 | 원래 사용하던 것 |
| localtunnel | ⚠️ 불안정 | 비밀번호 요구, 503 에러 빈번 |
| cloudflared | ✅ 작동 | Quick Tunnel 무료, 가입 불필요, 안정적 |

---

## 📝 코드 변경 완료

### 1. Cargo.toml
```diff
- bore-cli = "0.6"
+ # 터널링: localtunnel (npx로 외부 실행)
```
→ **bore-cli 의존성 제거됨**

### 2. src/main.rs (line ~481)
`start_tunnel()` 함수를 bore → localtunnel로 변경함

**하지만!** localtunnel도 불안정해서 **cloudflared로 다시 변경 필요**

---

## 🔧 TODO: cloudflared로 최종 변경

### Rust 코드에서 cloudflared 사용하도록 수정 필요:

```rust
// start_tunnel 함수를 이렇게 변경
async fn start_tunnel(local_port: u16, token: &str) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    use std::process::Stdio;
    use tokio::process::Command;
    use tokio::io::{BufReader, AsyncBufReadExt};
    
    let mut child = Command::new("cloudflared")
        .args(["tunnel", "--url", &format!("http://localhost:{}", local_port)])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    
    let stderr = child.stderr.take().ok_or("stderr 없음")?;
    let mut reader = BufReader::new(stderr).lines();
    
    // URL 파싱 (stderr에서 trycloudflare.com URL 찾기)
    let url = loop {
        if let Some(line) = reader.next_line().await? {
            if line.contains("trycloudflare.com") {
                // URL 추출: https://xxx.trycloudflare.com
                if let Some(start) = line.find("https://") {
                    let url_part = &line[start..];
                    if let Some(end) = url_part.find(|c: char| c.is_whitespace() || c == '|') {
                        break url_part[..end].to_string();
                    } else {
                        break url_part.trim().to_string();
                    }
                }
            }
        }
    };
    
    let external_token = generate_connection_token_with_url(&url, token);
    
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    
    Ok((url, external_token))
}
```

### cloudflared 설치 필요 (각 플랫폼별):
- **macOS:** `brew install cloudflared`
- **Windows:** `winget install Cloudflare.cloudflared`
- **Linux:** https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/

---

## 📁 파일 위치

```
~/work/web/mdflare/
├── agent-rust/
│   ├── Cargo.toml          # bore-cli 제거됨
│   ├── src/main.rs         # 터널링 코드 (localtunnel로 변경된 상태)
│   └── target/             # 빌드 결과
├── web/                    # 프론트엔드 (Cloudflare Pages 배포됨)
├── server/                 # 퍼블릭 API 서버
└── docs/                   # 문서
```

---

## 🚀 테스트 방법

```bash
# 1. cloudflared 설치 확인
cloudflared --version

# 2. 테스트 서버 띄우기
cd ~/work/web/mdflare/agent-rust
cargo run -- serve ~/Documents/MDFlare-Test

# 3. 또는 수동 테스트
node -e "require('http').createServer((q,s)=>{s.end('ok')}).listen(7779)"
cloudflared tunnel --url http://localhost:7779
```

---

## ⚠️ 주의사항

1. **cloudflared Quick Tunnel은 매번 URL이 바뀜**
   - 프로덕션에서는 Cloudflare 계정 연동 필요
   
2. **사용자에게 cloudflared 설치 요구됨**
   - 설치 가이드 문서화 필요
   
3. **미사용 함수 정리 필요**
   - `generate_connection_token_with_host()` 사용 안 됨 (warning)

---

## 📌 결론

**cloudflared Quick Tunnel이 최선의 선택:**
- 무료, 가입 불필요
- Cloudflare 인프라 (안정적)
- HTTPS 자동 지원
- 트래픽 비용 Cloudflare 부담

코드에서 localtunnel → cloudflared로 변경하면 완료!
