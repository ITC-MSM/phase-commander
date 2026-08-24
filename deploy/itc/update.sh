#!/usr/bin/env bash
set -euo pipefail

cd /opt/mtg/phase
git fetch origin platform
git reset --hard origin/platform
docker compose -f deploy/itc/docker-compose.yml pull
docker compose -f deploy/itc/docker-compose.yml up -d --remove-orphans
