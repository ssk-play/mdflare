#!/bin/bash
# 웹 빌드 → 로컬 개발 서버 (wrangler pages dev)
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WEB_DIR="$ROOT_DIR/web"

PORT=3000
for arg in "$@"; do
  case "$arg" in
    --port=*) PORT="${arg#--port=}" ;;
  esac
done

# 기존 포트 사용 중이면 종료
PID=$(lsof -ti :"$PORT" 2>/dev/null || true)
if [ -n "$PID" ]; then
  echo "⚠️  port $PORT 사용 중 (PID $PID) → 종료"
  kill "$PID" 2>/dev/null || true
  sleep 1
fi

echo "🔨 빌드 중..."
(cd "$WEB_DIR" && npm run build)

echo "🌐 로컬 서버 시작 (port $PORT)"
(cd "$WEB_DIR" && npx wrangler pages dev dist --port "$PORT")
