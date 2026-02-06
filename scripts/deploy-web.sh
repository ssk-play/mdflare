#!/bin/bash
# 웹 빌드 → Cloudflare Pages 배포 (프로덕션)
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WEB_DIR="$ROOT_DIR/web"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')

# 환경변수 체크
if [ -z "$CLOUDFLARE_API_TOKEN" ]; then
  # .env에서 로드 시도
  if [ -f "$ROOT_DIR/.env" ]; then
    export $(grep CLOUDFLARE_API_TOKEN "$ROOT_DIR/.env" | xargs)
  fi
  if [ -z "$CLOUDFLARE_API_TOKEN" ]; then
    echo "❌ CLOUDFLARE_API_TOKEN 환경변수 필요"
    echo "   export CLOUDFLARE_API_TOKEN=xxx 또는 .env 파일에 설정"
    exit 1
  fi
fi

echo "🌐 웹 v$VERSION 배포 시작"

# 1. 빌드
echo "🔨 빌드 중..."
(cd "$WEB_DIR" && npm run build)

# 2. Cloudflare Pages 배포
echo "📤 배포 중..."
(cd "$WEB_DIR" && npx wrangler pages deploy dist --project-name=mdflare --branch=main)

echo ""
echo "✅ v$VERSION 배포 완료 → mdflare.com"
