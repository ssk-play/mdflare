#!/bin/bash
# macOS 에이전트: 빌드 넘버 업 → 빌드 → Firebase Storage 배포
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION_FILE="$ROOT_DIR/VERSION"
BUILDS_FILE="$ROOT_DIR/builds.json"
CARGO_TOML="$ROOT_DIR/agent/Cargo.toml"
BUCKET="gs://markdownflare.firebasestorage.app/downloads/mac"

# 1. 메인 버전 읽기
MAIN_VERSION=$(cat "$VERSION_FILE" | tr -d '[:space:]')

# 2. 빌드 넘버 증가
BUILD=$(python3 -c "
import json, sys
f = '$BUILDS_FILE'
d = json.load(open(f))
d['mac'] = d.get('mac', 0) + 1
json.dump(d, open(f, 'w'))
print(d['mac'])
")

FULL_VERSION="$MAIN_VERSION.$BUILD"

echo "📦 v$FULL_VERSION (main: $MAIN_VERSION, build: $BUILD)"

# 3. Cargo.toml 버전 동기화
sed -i '' "s/^version = \".*\"/version = \"$FULL_VERSION\"/" "$CARGO_TOML"

# 4. 빌드
echo "🔨 빌드 중..."
source "$HOME/.cargo/env" 2>/dev/null || true
(cd "$ROOT_DIR/agent" && cargo build --release)

BINARY="$ROOT_DIR/agent/target/release/mdflare-agent"
ZIP="/tmp/MDFlare-Agent-${FULL_VERSION}-mac.zip"

# 5. zip 패키징
zip -j "$ZIP" "$BINARY"
SIZE=$(du -h "$ZIP" | cut -f1 | xargs)

echo "📤 업로드 중... ($SIZE)"

# 6. Firebase Storage 업로드
gsutil cp "$ZIP" "$BUCKET/MDFlare-Agent-${FULL_VERSION}-mac.zip"

# 7. meta.json 업데이트
echo "{\"version\":\"$FULL_VERSION\",\"size\":\"$SIZE\",\"date\":\"$(date +%Y-%m-%d)\"}" | \
  gsutil -h "Content-Type:application/json" cp - "$BUCKET/meta.json"

# 정리
rm -f "$ZIP"

echo ""
echo "✅ v$FULL_VERSION 배포 완료"
echo "   $BUCKET/MDFlare-Agent-${FULL_VERSION}-mac.zip"
