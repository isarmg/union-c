#!/bin/sh
set -eu
umask 077

: "${BACKUP_DIR:?set BACKUP_DIR to a private backup directory}"
: "${UNION_DATABASE_URL:?database URL is required}"
: "${AGE_RECIPIENT:?age recipient is required; unencrypted backups are refused}"

command -v pg_dump >/dev/null
command -v age >/dev/null
command -v tar >/dev/null

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ENV_FILE=${UNION_ENV_FILE:-/etc/union/union.env}
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
WORK=$(mktemp -d)
SERVICE_NAME=${UNION_SERVICE_NAME:-union}
WAS_ACTIVE=0

cleanup() {
    if [ "$WAS_ACTIVE" -eq 1 ]; then
        systemctl start "$SERVICE_NAME"
    fi
    rm -rf "$WORK"
}
trap cleanup EXIT HUP INT TERM

if command -v systemctl >/dev/null && systemctl is-active --quiet "$SERVICE_NAME"; then
    systemctl stop "$SERVICE_NAME"
    WAS_ACTIVE=1
elif [ "${UNION_ASSUME_QUIESCED:-0}" != "1" ]; then
    echo "Union must be stopped for a consistent database/files backup." >&2
    echo "Stop $SERVICE_NAME or set UNION_ASSUME_QUIESCED=1 after quiescing writes externally." >&2
    exit 1
fi

mkdir -p "$BACKUP_DIR"
pg_dump --dbname "$UNION_DATABASE_URL" --format=custom --file="$WORK/database.dump"
tar -C "$ROOT" -cf "$WORK/runtime-files.tar" blog/data/files ram/data/files
cp "$ENV_FILE" "$WORK/union.env"
if [ -f "$ROOT/union/data/union-config.json" ]; then
    cp "$ROOT/union/data/union-config.json" "$WORK/union-config.json"
fi
if [ -f "$ROOT/union/data/union.secret" ]; then
    cp "$ROOT/union/data/union.secret" "$WORK/union.secret"
fi

tar -C "$WORK" -cf - . | age -r "$AGE_RECIPIENT" -o "$BACKUP_DIR/union-$STAMP.tar.age"
sha256sum "$BACKUP_DIR/union-$STAMP.tar.age" > "$BACKUP_DIR/union-$STAMP.tar.age.sha256"
echo "Encrypted backup created: $BACKUP_DIR/union-$STAMP.tar.age"
