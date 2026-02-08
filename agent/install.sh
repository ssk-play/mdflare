#!/bin/bash
# MDFlare Agent 설치 스크립트

APP_NAME="MDFlare Agent.app"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/$APP_NAME"
DEST="/Applications/$APP_NAME"

if [ ! -d "$SRC" ]; then
    echo "오류: $APP_NAME 을 찾을 수 없습니다."
    exit 1
fi

echo "MDFlare Agent 설치 중..."

# 기존 앱 종료
pkill -f "mdflare-agent" 2>/dev/null

# 복사
rm -rf "$DEST"
cp -R "$SRC" "$DEST"

# Gatekeeper quarantine 제거
xattr -cr "$DEST"

echo "설치 완료! 앱을 실행합니다."
open "$DEST"
