#!/bin/bash
# MDFlare Agent 배포 스크립트
# 사용법: ./deploy.sh [major|minor|patch]
# 기본: patch

set -e

AGENT_DIR="$(cd "$(dirname "$0")/MDFlareAgent" && pwd)"
BUILD_DIR="$(cd "$(dirname "$0")/build" && pwd)"
WEB_DIR="$(cd "$(dirname "$0")/../web" && pwd)"
PLIST="$AGENT_DIR/Sources/Info.plist"
DOWNLOAD_JSX="$WEB_DIR/src/pages/Download.jsx"
BUCKET="markdownflare.firebasestorage.app"

# 현재 버전 읽기
CURRENT=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$PLIST")
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

# 버전 올리기
BUMP=${1:-patch}
case $BUMP in
  major) MAJOR=$((MAJOR+1)); MINOR=0; PATCH=0 ;;
  minor) MINOR=$((MINOR+1)); PATCH=0 ;;
  patch) PATCH=$((PATCH+1)) ;;
esac

NEW_VERSION="$MAJOR.$MINOR.$PATCH"
echo "🏷️  $CURRENT → $NEW_VERSION"

# Info.plist 업데이트
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $NEW_VERSION" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $((MAJOR*10000 + MINOR*100 + PATCH))" "$PLIST"

# 빌드
echo "🔨 빌드 중..."
cd "$AGENT_DIR"
swiftc -o MDFlareAgent Sources/main.swift -framework AppKit -framework Foundation -swift-version 5 2>&1 | grep -v warning || true

# .app 번들
echo "📦 패키징..."
mkdir -p "$BUILD_DIR/MDFlare Agent.app/Contents/MacOS"
mkdir -p "$BUILD_DIR/MDFlare Agent.app/Contents/Resources"
cp "$AGENT_DIR/MDFlareAgent" "$BUILD_DIR/MDFlare Agent.app/Contents/MacOS/"
cp "$PLIST" "$BUILD_DIR/MDFlare Agent.app/Contents/"

# zip
ZIP_NAME="MDFlare-Agent-${NEW_VERSION}-mac.zip"
cd "$BUILD_DIR"
rm -f "$ZIP_NAME"
zip -r "$ZIP_NAME" "MDFlare Agent.app"
ZIP_SIZE=$(du -h "$ZIP_NAME" | cut -f1 | tr -d ' ')
echo "📁 $ZIP_NAME ($ZIP_SIZE)"

# Firebase Storage 업로드
echo "☁️  업로드 중..."
ENCODED_PATH="downloads%2Fmac%2F${ZIP_NAME}"
RESPONSE=$(curl -s -X POST \
  "https://firebasestorage.googleapis.com/v0/b/${BUCKET}/o?uploadType=media&name=${ENCODED_PATH}" \
  -H "Content-Type: application/zip" \
  --data-binary @"${ZIP_NAME}")

TOKEN=$(echo "$RESPONSE" | grep -o '"downloadTokens": "[^"]*"' | cut -d'"' -f4)

if [ -z "$TOKEN" ]; then
  echo "❌ 업로드 실패"
  echo "$RESPONSE"
  exit 1
fi

DOWNLOAD_URL="https://firebasestorage.googleapis.com/v0/b/${BUCKET}/o/${ENCODED_PATH}?alt=media&token=${TOKEN}"
echo "✅ 업로드 완료: $DOWNLOAD_URL"

# Download.jsx 업데이트
echo "🌐 다운로드 페이지 업데이트..."
# Python으로 안전하게 치환 (sed의 특수문자 문제 회피)
python3 -c "
import re, sys
with open('$DOWNLOAD_JSX', 'r') as f:
    content = f.read()
# URL 교체
content = re.sub(
    r'href=\"https://firebasestorage\.googleapis\.com/v0/b/markdownflare\.firebasestorage\.app/o/downloads[^\"]*\"',
    'href=\"${DOWNLOAD_URL}\"',
    content
)
# 사이즈 교체
content = re.sub(r'다운로드 \([^)]*\)', '다운로드 (${ZIP_SIZE})', content)
# 버전 교체
content = re.sub(r'v\d+\.\d+\.\d+', 'v${NEW_VERSION}', content)
with open('$DOWNLOAD_JSX', 'w') as f:
    f.write(content)
"

echo ""
echo "🎉 v${NEW_VERSION} 배포 준비 완료!"
echo "   웹 배포는 별도로: cd web && npm run build && wrangler pages deploy dist"
