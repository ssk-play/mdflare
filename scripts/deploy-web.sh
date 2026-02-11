#!/bin/bash
# 웹 빌드 → Cloudflare Pages 배포 (프로덕션)
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')

# 환경변수 체크
if [ -z "$CLOUDFLARE_API_TOKEN" ]; then
  if [ -f "$ROOT_DIR/.env" ]; then
    export $(grep CLOUDFLARE_API_TOKEN "$ROOT_DIR/.env" | xargs)
  fi
  if [ -z "$CLOUDFLARE_API_TOKEN" ]; then
    echo "❌ CLOUDFLARE_API_TOKEN 환경변수 필요"
    exit 1
  fi
fi

echo "🌐 웹 v$VERSION 배포 시작"

# 1. cloud 패키지 빌드 & 배포
echo "☁️ cloud.mdflare.com 배포..."
(cd "$ROOT_DIR/packages/cloud" && npm run build)
(cd "$ROOT_DIR/packages/cloud" && npx wrangler pages deploy dist --project-name=mdflare-cloud --branch=main)

# 2. private 패키지 빌드 & 배포
echo "🔐 private vault 배포..."
(cd "$ROOT_DIR/packages/private" && npm run build)
(cd "$ROOT_DIR/packages/private" && npx wrangler pages deploy dist --project-name=mdflare-private --branch=main)

# 3. landing 페이지 배포
echo "🏠 landing 배포..."
(cd "$ROOT_DIR/packages/landing" && npm run build)
(cd "$ROOT_DIR/packages/landing" && npx wrangler pages deploy dist --project-name=mdflare --branch=main)

echo ""
echo "✅ v$VERSION 배포 완료"
echo "   - cloud.mdflare.com"
echo "   - private vault"
echo "   - mdflare.com (landing)"
