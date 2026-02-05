#!/bin/bash
# MDFlare 배포 스크립트
# 사용법: ./deploy.sh "커밋 메시지"

set -e
cd "$(dirname "$0")"

MSG="$1"
if [ -z "$MSG" ]; then
  echo "❌ 사용법: ./deploy.sh \"커밋 메시지\""
  exit 1
fi

# 1. 변경사항 커밋
echo "📝 커밋: $MSG"
git add -A
git commit -m "$MSG"

# 2. 빌드 (커밋 메시지가 footer에 반영됨)
echo "🔨 빌드..."
cd web && npx vite build && cd ..

# 3. 빌드 결과물 포함해서 커밋 수정
echo "📦 빌드 반영..."
git add -A
git commit --amend --no-edit

# 4. Push
echo "🚀 Push..."
git push

# 5. 배포
echo "☁️ 배포..."
cd web
CLOUDFLARE_API_TOKEN=$(grep CLOUDFLARE_API_TOKEN ../.env | cut -d= -f2) npx wrangler pages deploy ./dist --project-name=mdflare

echo "✅ 완료!"
