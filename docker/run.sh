#!/usr/bin/env bash
# UNTESTED. Build the image and start the container. Run from anywhere: ./docker/run.sh
set -eu
here=$(dirname "$(readlink -f "$0")")
mkdir -p "$here/../out"
docker build -f "$here/Dockerfile" -t claude-desk "$here/.."
docker rm -f claude-desk >/dev/null 2>&1 || true
# /run/user/1000 is a tmpfs owned by the desk user; ./out receives screenshots (desk shot /out/x.png)
docker run -d --name claude-desk --tmpfs /run/user/1000:uid=1000,gid=1000,mode=700 \
  -v claude-desk-profile:/home/desk/.local/share -v "$here/../out:/out" claude-desk
echo "container up. Use: docker/desk-docker start && docker/desk-docker chromium https://example.com && docker/desk-docker shot /out/a.png"
