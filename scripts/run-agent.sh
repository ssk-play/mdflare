#!/bin/bash
# 에이전트 빌드 → .app 번들 → quarantine 제거 → 실행
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')

source "$HOME/.cargo/env" 2>/dev/null || true

PROFILE="debug"
for arg in "$@"; do
  case "$arg" in
    --release) PROFILE="release" ;;
    --debug)   PROFILE="debug" ;;
  esac
done

CARGO_FLAGS=""
if [ "$PROFILE" = "release" ]; then
  CARGO_FLAGS="--release"
fi

echo "🔨 빌드 중... ($PROFILE)"
(cd "$ROOT_DIR/agent" && cargo build $CARGO_FLAGS)

BINARY="$ROOT_DIR/agent/target/$PROFILE/mdflare-agent"
APP_DIR="/Applications/MDFlare Agent.app"

# 기존 앱 종료
pkill -f "mdflare-agent" 2>/dev/null || true
sleep 1

# .app 번들 생성
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

sed -e "s/<string>1\.0\.5</<string>$VERSION</" \
  "$ROOT_DIR/agent/macos/Info.plist" > "$APP_DIR/Contents/Info.plist"

cp "$BINARY" "$APP_DIR/Contents/MacOS/mdflare-agent"
cp "$ROOT_DIR/agent/macos/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"

# quarantine 제거 + URL scheme 등록
xattr -cr "$APP_DIR"
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister -f "$APP_DIR"

echo "📦 /Applications에 설치 (URL scheme 중복 등록 방지)"
echo "🚀 실행 중..."
open "$APP_DIR"
