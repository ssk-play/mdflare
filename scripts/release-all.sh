#!/bin/bash
# 전체 릴리스: 버전 bump → 웹 배포 → macOS 에이전트 배포
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPTS="$ROOT_DIR/scripts"

PROFILE="release"
for arg in "$@"; do
  case "$arg" in
    --release) PROFILE="release" ;;
    --debug)   PROFILE="debug" ;;
  esac
done

echo "🚀 전체 릴리스 시작 ($PROFILE)"
echo ""

# 1. 버전 bump
echo "━━━ [1/3] 버전 bump ━━━"
bash "$SCRIPTS/bump-patch.sh"
VERSION=$(cat "$ROOT_DIR/VERSION" | tr -d '[:space:]')
echo ""

# 2. 웹 배포
echo "━━━ [2/3] 웹 배포 ━━━"
bash "$SCRIPTS/deploy-web.sh"
echo ""

# 3. macOS 에이전트 배포
echo "━━━ [3/3] macOS 에이전트 배포 ━━━"
bash "$SCRIPTS/release-mac.sh" --$PROFILE
echo ""

echo "🎉 v$VERSION 전체 릴리스 완료!"
