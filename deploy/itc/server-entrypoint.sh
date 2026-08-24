#!/bin/sh
set -eu

: "${PHASE_DATA_DIR:=/var/lib/phase-server}"
export PHASE_DATA_DIR
mkdir -p "$PHASE_DATA_DIR"
cp /opt/phase-seed/card-data.json "$PHASE_DATA_DIR/card-data.json"
cp /opt/phase-seed/draft-pools.json "$PHASE_DATA_DIR/draft-pools.json"
chown -R phase:phase "$PHASE_DATA_DIR"
exec gosu phase phase-server "$@"

