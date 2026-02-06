#!/bin/bash
# MDFlare Agent 설치 및 실행
cd "$(dirname "$0")"
xattr -cr mdflare-agent
chmod +x mdflare-agent
./mdflare-agent
