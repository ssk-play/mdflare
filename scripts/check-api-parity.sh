#!/bin/bash
# API 인터페이스 통일 검사
# Cloud와 Private Vault가 동일한 API를 구현했는지 확인

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

ERRORS=0

# 필수 API 엔드포인트 정의
REQUIRED_APIS=(
  "GET /api/files"
  "GET /api/file/:path"
  "PUT /api/file/:path"
  "DELETE /api/file/:path"
  "POST /api/rename"
)

echo "🔍 API 인터페이스 통일 검사"
echo "=========================="

# Cloud (Cloudflare Functions) 검사
echo -e "\n📁 Cloud API 검사..."
CLOUD_DIR="web/functions/api/[userId]"

check_cloud() {
  local method=$1
  local endpoint=$2
  
  case "$endpoint" in
    "/api/files")
      [ -f "$CLOUD_DIR/files.js" ] && return 0
      ;;
    "/api/file/:path")
      [ -f "$CLOUD_DIR/file/[[path]].js" ] && return 0
      ;;
    "/api/rename")
      [ -f "$CLOUD_DIR/rename.js" ] && return 0
      ;;
  esac
  return 1
}

# Private Vault (Rust Agent) 검사
echo -e "\n🦀 Private Vault API 검사..."
RUST_FILE="agent-rust/src/main.rs"

check_private_vault() {
  local method=$1
  local endpoint=$2
  
  case "$endpoint" in
    "/api/files")
      grep -q 'route.*"/api/files"' "$RUST_FILE" && return 0
      ;;
    "/api/file/:path")
      grep -q 'route.*"/api/file/\*path"' "$RUST_FILE" && return 0
      ;;
    "/api/rename")
      grep -q 'route.*"/api/rename"' "$RUST_FILE" && return 0
      ;;
  esac
  return 1
}

# 검사 실행
echo -e "\n결과:"
echo "------"

for api in "${REQUIRED_APIS[@]}"; do
  method=$(echo "$api" | cut -d' ' -f1)
  endpoint=$(echo "$api" | cut -d' ' -f2)
  
  cloud_ok=false
  pv_ok=false
  
  if check_cloud "$method" "$endpoint"; then
    cloud_ok=true
  fi
  
  if check_private_vault "$method" "$endpoint"; then
    pv_ok=true
  fi
  
  if $cloud_ok && $pv_ok; then
    echo -e "${GREEN}✅ $api${NC}"
  else
    echo -e "${RED}❌ $api${NC}"
    [ "$cloud_ok" = false ] && echo -e "   ${RED}└─ Cloud 미구현${NC}"
    [ "$pv_ok" = false ] && echo -e "   ${RED}└─ Private Vault 미구현${NC}"
    ERRORS=$((ERRORS + 1))
  fi
done

echo ""
if [ $ERRORS -gt 0 ]; then
  echo -e "${RED}❌ $ERRORS개 API 불일치 발견!${NC}"
  exit 1
else
  echo -e "${GREEN}✅ 모든 API 인터페이스 통일됨${NC}"
  exit 0
fi
