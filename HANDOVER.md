# MDFlare Handover - 2026-02-06

## ✅ 완료: cloudflared Quick Tunnel 전환

**bore.pub 다운 → localtunnel 불안정 → cloudflared Quick Tunnel로 변경 완료!**

### 변경 사항

1. **agent/src/main.rs**
   - `start_tunnel()`: localtunnel → cloudflared
   - stderr에서 trycloudflare.com URL 파싱
   - tokio::spawn에서 stderr 계속 drain하여 터널 유지

2. **web/functions/_tunnel/[[path]].js**
   - trycloudflare.com은 https로 연결
   - Host 헤더 설정 (Cloudflare 터널 필수)

3. **web/src/pages/Landing.jsx**
   - https:// URL도 처리하도록 regex 수정

4. **미사용 코드 정리**
   - unused imports 제거 (delete, put)
   - unused variable 제거 (root_items)
   - unused function 제거 (generate_connection_token_with_host)

### 테스트 완료
- cloudflared Quick Tunnel 정상 작동
- 외부 접속 토큰으로 웹에서 연결 성공

---

## 📌 남은 작업

1. **main 브랜치 머지** → mdflare.com 배포
2. **사용자 가이드** - cloudflared 설치 안내 문서화
   - macOS: `brew install cloudflared`
   - Windows: `winget install Cloudflare.cloudflared`
3. **GitHub Dependabot 취약점** - 1 moderate (확인 필요)

---

## 📁 브랜치

- `feature/tunneling` - cloudflared 변경 완료 (현재)
- `main` - 아직 머지 안 됨
