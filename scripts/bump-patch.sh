#!/bin/bash
# 패치 버전 +1 → VERSION, Cargo.toml, package.json 업데이트 → 커밋
set -e

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION_FILE="$ROOT_DIR/VERSION"

CURRENT=$(cat "$VERSION_FILE" | tr -d '[:space:]')
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"
NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"

echo "$NEW_VERSION" > "$VERSION_FILE"
sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$ROOT_DIR/agent/Cargo.toml"
sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT_DIR/package.json"
sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT_DIR/web/package.json"

echo "📦 $CURRENT → $NEW_VERSION"

cd "$ROOT_DIR"
git add VERSION agent/Cargo.toml package.json web/package.json
git commit -m "chore: bump version to $NEW_VERSION"
