#!/bin/bash
# 웹 빌드 → Cloudflare Pages 배포 (프로덕션)
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WEB_DIR="$ROOT_DIR/web"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')

echo "🌐 웹 v$VERSION 배포 시작"

# 1. 빌드
echo "🔨 빌드 중..."
(cd "$WEB_DIR" && npm run build)

# 2. Cloudflare Pages 배포
echo "📤 배포 중..."
(cd "$WEB_DIR" && npx wrangler pages deploy dist --project-name=mdflare --branch=main)

echo ""
echo "✅ v$VERSION 배포 완료 → mdflare.com"
