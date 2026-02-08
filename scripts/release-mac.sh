#!/bin/bash
# macOS 에이전트: 빌드 → .app 번들 → Firebase Storage 배포
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
APP_DIR="/tmp/MDFlare Agent.app"
ZIP="/tmp/MDFlare-Agent-${VERSION}-mac.zip"

# 2. .app 번들 생성
echo "📁 .app 번들 생성 중..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"

# Info.plist 복사 + 버전 업데이트
sed -e "s/<string>1\.0\.5</<string>$VERSION</" \
  "$ROOT_DIR/agent/macos/Info.plist" > "$APP_DIR/Contents/Info.plist"

cp "$BINARY" "$APP_DIR/Contents/MacOS/mdflare-agent"

# 3. install.sh 복사 + zip 패키징
cp "$ROOT_DIR/agent/install.sh" /tmp/install.sh
(cd /tmp && zip -r "$ZIP" "MDFlare Agent.app" install.sh)
SIZE=$(du -h "$ZIP" | cut -f1 | xargs)

echo "📤 업로드 중... ($SIZE)"

# 4. Firebase Storage 업로드
gsutil cp "$ZIP" "$BUCKET/MDFlare-Agent-${VERSION}-mac.zip"

# 5. meta.json 업데이트
echo "{\"version\":\"$VERSION\",\"size\":\"$SIZE\",\"date\":\"$(date +%Y-%m-%d)\"}" | \
  gsutil -h "Content-Type:application/json" cp - "$BUCKET/meta.json"

# 정리
rm -rf "$APP_DIR" "$ZIP" /tmp/install.sh

echo ""
echo "✅ v$VERSION 배포 완료"
echo "   $BUCKET/MDFlare-Agent-${VERSION}-mac.zip"
