#!/bin/bash
# macOS 에이전트 패치 버전 업 → 빌드 → Firebase Storage 배포
set -e

AGENT_DIR="$(cd "$(dirname "$0")/../agent" && pwd)"
CARGO_TOML="$AGENT_DIR/Cargo.toml"
BUCKET="gs://markdownflare.firebasestorage.app/downloads/mac"

# 1. 현재 버전 읽기
CURRENT=$(grep '^version' "$CARGO_TOML" | head -1 | sed 's/.*"\(.*\)"/\1/')
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"
NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"

echo "📦 $CURRENT → $NEW_VERSION"

# 2. Cargo.toml 버전 업데이트
sed -i '' "s/^version = \"$CURRENT\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"

# 3. 빌드
echo "🔨 빌드 중..."
source "$HOME/.cargo/env" 2>/dev/null || true
(cd "$AGENT_DIR" && cargo build --release)

BINARY="$AGENT_DIR/target/release/mdflare-agent"
ZIP="/tmp/MDFlare-Agent-${NEW_VERSION}-mac.zip"

# 4. zip 패키징
zip -j "$ZIP" "$BINARY"
SIZE=$(du -h "$ZIP" | cut -f1 | xargs)

echo "📤 업로드 중... ($SIZE)"

# 5. Firebase Storage 업로드
gsutil cp "$ZIP" "$BUCKET/MDFlare-Agent-${NEW_VERSION}-mac.zip"

# 6. meta.json 업데이트
echo "{\"version\":\"$NEW_VERSION\",\"size\":\"$SIZE\",\"date\":\"$(date +%Y-%m-%d)\"}" | \
  gsutil -h "Content-Type:application/json" cp - "$BUCKET/meta.json"

# 정리
rm -f "$ZIP"

echo ""
echo "✅ v$NEW_VERSION 배포 완료"
echo "   $BUCKET/MDFlare-Agent-${NEW_VERSION}-mac.zip"
