#!/bin/bash
# macOS 에이전트: 빌드 → Firebase Storage 배포
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')
BUCKET="gs://markdownflare.firebasestorage.app/downloads/mac"

# gsutil 체크
if ! command -v gsutil &> /dev/null; then
  echo "❌ gsutil 필요: brew install google-cloud-sdk"
  exit 1
fi

echo "📦 v$VERSION 빌드 시작"

# 1. 빌드
echo "🔨 빌드 중..."
source "$HOME/.cargo/env" 2>/dev/null || true
(cd "$ROOT_DIR/agent" && cargo build --release)

BINARY="$ROOT_DIR/agent/target/release/mdflare-agent"
ZIP="/tmp/MDFlare-Agent-${VERSION}-mac.zip"

# 2. zip 패키징
zip -j "$ZIP" "$BINARY" "$ROOT_DIR/agent/install.command"
SIZE=$(du -h "$ZIP" | cut -f1 | xargs)

echo "📤 업로드 중... ($SIZE)"

# 3. Firebase Storage 업로드
gsutil cp "$ZIP" "$BUCKET/MDFlare-Agent-${VERSION}-mac.zip"

# 4. meta.json 업데이트
echo "{\"version\":\"$VERSION\",\"size\":\"$SIZE\",\"date\":\"$(date +%Y-%m-%d)\"}" | \
  gsutil -h "Content-Type:application/json" cp - "$BUCKET/meta.json"

# 정리
rm -f "$ZIP"

echo ""
echo "✅ v$VERSION 배포 완료"
echo "   $BUCKET/MDFlare-Agent-${VERSION}-mac.zip"
