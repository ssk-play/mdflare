#!/bin/bash
# 웹 빌드 → Cloudflare Pages 배포
# 사용법: ./scripts/deploy-web.sh [dev|prod]
set -e

ENV="${1:-dev}"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WEB_DIR="$ROOT_DIR/web"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')

case "$ENV" in
  dev)
    BRANCH="dev"
    URL="dev.mdflare.com"
    ;;
  prod)
    BRANCH="main"
    URL="mdflare.com"
    ;;
  *)
    echo "사용법: $0 [dev|prod]"
    exit 1
    ;;
esac

echo "🌐 웹 v$VERSION → $URL 배포 시작"

# 1. 빌드
echo "🔨 빌드 중..."
(cd "$WEB_DIR" && npm run build)

# 2. Cloudflare Pages 배포
echo "📤 배포 중..."
(cd "$WEB_DIR" && npx wrangler pages deploy dist --project-name=mdflare --branch="$BRANCH")

echo ""
echo "✅ v$VERSION 배포 완료 → $URL"
