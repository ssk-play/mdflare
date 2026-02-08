#!/bin/bash
set -e
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

# 기존 앱 종료 및 교체
if [ -d "$DEST" ]; then
    pkill -f "mdflare-agent" 2>/dev/null || true
    rm -rf "$DEST"
fi
cp -R "$SRC" "$DEST"

# Gatekeeper quarantine 제거
xattr -cr "$DEST"

echo "설치 완료! 앱을 실행합니다."
open "$DEST"
