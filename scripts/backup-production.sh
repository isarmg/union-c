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
trap 'rm -rf "$WORK"' EXIT HUP INT TERM

mkdir -p "$BACKUP_DIR"
pg_dump --dbname "$UNION_DATABASE_URL" --format=custom --file="$WORK/database.dump"
tar -C "$ROOT" -cf "$WORK/runtime-files.tar" data/blog/files data/ram/files
cp "$ENV_FILE" "$WORK/union.env"
if [ -f "$ROOT/data/union.secret" ]; then
    cp "$ROOT/data/union.secret" "$WORK/union.secret"
fi

tar -C "$WORK" -cf - . | age -r "$AGE_RECIPIENT" -o "$BACKUP_DIR/union-$STAMP.tar.age"
sha256sum "$BACKUP_DIR/union-$STAMP.tar.age" > "$BACKUP_DIR/union-$STAMP.tar.age.sha256"
echo "Encrypted backup created: $BACKUP_DIR/union-$STAMP.tar.age"
