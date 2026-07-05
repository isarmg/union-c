#!/bin/sh
set -eu

umask 027

SOURCE_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DEPLOY_ROOT=${DEPLOY_ROOT:-/opt/union}
SERVICE_USER=${SERVICE_USER:-union}
SERVICE_GROUP=${SERVICE_GROUP:-$SERVICE_USER}
ENV_DIR=${ENV_DIR:-/etc/union}
ENV_FILE=${ENV_FILE:-$ENV_DIR/union.env}
SYSTEMD_DIR=${SYSTEMD_DIR:-/etc/systemd/system}
TMPFILES_DIR=${TMPFILES_DIR:-/etc/tmpfiles.d}
LOGROTATE_DIR=${LOGROTATE_DIR:-/etc/logrotate.d}
DRY_RUN=0
START_AFTER_INSTALL=0
COMMAND=

usage() {
    cat <<'USAGE'
Usage: scripts/deploy-linux.sh [--dry-run] [--start] COMMAND

Commands:
  check       Check Linux, source layout, and required build/deploy commands.
  configure   Write /etc/union/union.env from UNION_* variables or prompts.
  build       Build and test the four independent projects in the source tree.
  install     Stage source in /opt/union, build, and install Linux service files.
  start       Enable and start union.service.
  verify      Check service state and local health endpoints.
  all         Run check, configure, install, and optionally start with --start.

Overrides:
  DEPLOY_ROOT, SERVICE_USER, SERVICE_GROUP, ENV_DIR, ENV_FILE
  UNION_DATABASE_URL, UNION_SECRET_KEY, UNION_BOOTSTRAP_PASSWORD
  UNION_RAM_PUBLIC_URL, UNION_RETENTION_DAYS, PUBLIC_SITE_URL
  UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS, RUST_LOG

Examples:
  ./scripts/deploy-linux.sh check
  sudo ./scripts/deploy-linux.sh configure
  sudo ./scripts/deploy-linux.sh install --start
  DEPLOY_ROOT=/srv/union sudo --preserve-env=DEPLOY_ROOT ./scripts/deploy-linux.sh install
USAGE
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

note() {
    printf '%s\n' "$*"
}

run() {
    if [ "$DRY_RUN" -eq 1 ]; then
        printf 'dry-run:'
        printf ' %s' "$@"
        printf '\n'
        return 0
    fi
    "$@"
}

run_in() {
    directory=$1
    shift
    if [ "$DRY_RUN" -eq 1 ]; then
        printf 'dry-run: cd %s &&' "$directory"
        printf ' %s' "$@"
        printf '\n'
        return 0
    fi
    (
        cd "$directory"
        "$@"
    )
}

require_root() {
    if [ "$DRY_RUN" -eq 0 ] && [ "$(id -u)" -ne 0 ]; then
        die "this command must run as root"
    fi
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

validate_settings() {
    [ "$(uname -s)" = Linux ] || die "only Linux is supported"
    case "$DEPLOY_ROOT" in
        /*) ;;
        *) die "DEPLOY_ROOT must be an absolute path" ;;
    esac
    [ "$DEPLOY_ROOT" != / ] || die "DEPLOY_ROOT must not be /"
    case "$SERVICE_USER" in
        -*|*[!a-zA-Z0-9_-]*|'') die "invalid SERVICE_USER" ;;
    esac
    case "$SERVICE_GROUP" in
        -*|*[!a-zA-Z0-9_-]*|'') die "invalid SERVICE_GROUP" ;;
    esac
    for path_setting in \
        "ENV_DIR:$ENV_DIR" \
        "ENV_FILE:$ENV_FILE" \
        "SYSTEMD_DIR:$SYSTEMD_DIR" \
        "TMPFILES_DIR:$TMPFILES_DIR" \
        "LOGROTATE_DIR:$LOGROTATE_DIR"
    do
        setting_name=${path_setting%%:*}
        setting_value=${path_setting#*:}
        case "$setting_value" in
            /*) ;;
            *) die "$setting_name must be an absolute path" ;;
        esac
    done
}

check_layout() {
    for path in \
        back/source/package.json \
        union/source/Cargo.toml \
        ram/source/Cargo.toml \
        blog/source/package.json \
        config/systemd/union.service \
        config/tmpfiles-union.conf \
        config/logrotate-union \
        .env.production.example
    do
        [ -f "$SOURCE_ROOT/$path" ] || die "required project file is missing: $path"
    done
}

check_commands() {
    for command_name in cargo npm node tar install openssl sed find
    do
        require_command "$command_name"
    done
}

check_all() {
    validate_settings
    check_layout
    check_commands
    note "Linux and project layout: OK"
    note "Rust: $(rustc --version 2>/dev/null || printf unavailable)"
    note "Cargo: $(cargo --version)"
    note "Node: $(node --version)"
    note "npm: $(npm --version)"
    note "deploy root: $DEPLOY_ROOT"
    note "service account: $SERVICE_USER:$SERVICE_GROUP"
}

read_setting() {
    variable_name=$1
    prompt=$2
    default_value=$3
    eval "current_value=\${$variable_name:-}"
    if [ -n "$current_value" ]; then
        printf '%s' "$current_value"
        return 0
    fi
    if [ -t 0 ]; then
        if [ -n "$default_value" ]; then
            printf '%s [%s]: ' "$prompt" "$default_value" >&2
        else
            printf '%s: ' "$prompt" >&2
        fi
        IFS= read -r entered
        printf '%s' "${entered:-$default_value}"
        return 0
    fi
    printf '%s' "$default_value"
}

reject_env_value() {
    label=$1
    value=$2
    case "$value" in
        *[[:cntrl:]]*) die "$label must be a single line without control characters" ;;
    esac
}

write_env_line() {
    key=$1
    value=$2
    reject_env_value "$key" "$value"
    escaped=$(printf '%s' "$value" | sed 's/\\/\\\\/g; s/"/\\"/g')
    printf '%s="%s"\n' "$key" "$escaped"
}

configure_environment() {
    require_root
    require_command openssl
    require_command install
    ensure_service_account

    database_url=$(read_setting UNION_DATABASE_URL "PostgreSQL URL" "")
    [ -n "$database_url" ] || die "UNION_DATABASE_URL is required"
    case "$database_url" in
        postgres://*|postgresql://*) ;;
        *) die "UNION_DATABASE_URL must use postgres:// or postgresql://" ;;
    esac

    secret_key=${UNION_SECRET_KEY:-}
    if [ -z "$secret_key" ]; then
        secret_key=$(openssl rand -base64 32 | tr -d '\n')
        note "generated a new UNION_SECRET_KEY"
    fi
    decoded_size=$(printf '%s' "$secret_key" | openssl base64 -d -A 2>/dev/null | wc -c | tr -d ' ')
    [ "$decoded_size" = 32 ] || die "UNION_SECRET_KEY must decode to exactly 32 bytes"
    secret_key_id=${UNION_SECRET_KEY_ID:-primary}
    case "$secret_key_id" in
        *[!a-zA-Z0-9_-]*|'') die "UNION_SECRET_KEY_ID must contain only ASCII letters, digits, '-' or '_'" ;;
    esac
    [ "${#secret_key_id}" -le 64 ] || die "UNION_SECRET_KEY_ID must be at most 64 characters"

    bootstrap_password=${UNION_BOOTSTRAP_PASSWORD:-}
    if [ -z "$bootstrap_password" ]; then
        bootstrap_password=$(openssl rand -base64 24 | tr -d '\n')
        note "generated a one-time UNION_BOOTSTRAP_PASSWORD"
    fi
    [ "${#bootstrap_password}" -ge 12 ] || die "UNION_BOOTSTRAP_PASSWORD must be at least 12 characters"

    ram_url=$(read_setting UNION_RAM_PUBLIC_URL "ram public HTTPS URL" "https://files.home.lan")
    site_url=$(read_setting PUBLIC_SITE_URL "blog public HTTPS URL" "https://home.lan")
    retention_days=${UNION_RETENTION_DAYS:-90}
    require_local_static=${UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS:-0}
    rust_log=${RUST_LOG:-union=info,tower_http=info}

    case "$ram_url" in https://*) ;; *) die "UNION_RAM_PUBLIC_URL must use https://" ;; esac
    case "$site_url" in https://*) ;; *) die "PUBLIC_SITE_URL must use https://" ;; esac
    case "$retention_days" in *[!0-9]*|'') die "UNION_RETENTION_DAYS must be numeric" ;; esac
    case "$require_local_static" in 0|1|true|false|TRUE|FALSE|yes|no|YES|NO) ;; *) die "UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS must be boolean" ;; esac

    for env_setting in \
        "UNION_DATABASE_URL:$database_url" \
        "UNION_SECRET_KEY:$secret_key" \
        "UNION_SECRET_KEY_ID:$secret_key_id" \
        "UNION_BOOTSTRAP_PASSWORD:$bootstrap_password" \
        "UNION_RAM_PUBLIC_URL:$ram_url" \
        "UNION_RETENTION_DAYS:$retention_days" \
        "PUBLIC_SITE_URL:$site_url" \
        "UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS:$require_local_static" \
        "RUST_LOG:$rust_log"
    do
        setting_name=${env_setting%%:*}
        setting_value=${env_setting#*:}
        reject_env_value "$setting_name" "$setting_value"
    done

    if [ "$DRY_RUN" -eq 1 ]; then
        note "dry-run: write protected environment file to $ENV_FILE"
        return 0
    fi

    run install -d -o root -g "$SERVICE_GROUP" -m 0750 "$ENV_DIR"
    temporary=$(mktemp "$ENV_DIR/.union.env.XXXXXX")
    trap 'rm -f "$temporary"' EXIT HUP INT TERM
    {
        write_env_line UNION_ENV production
        write_env_line UNION_DATABASE_URL "$database_url"
        write_env_line UNION_SECRET_KEY "$secret_key"
        write_env_line UNION_SECRET_KEY_ID "$secret_key_id"
        write_env_line UNION_BOOTSTRAP_PASSWORD "$bootstrap_password"
        write_env_line UNION_RAM_PUBLIC_URL "$ram_url"
        write_env_line UNION_RETENTION_DAYS "$retention_days"
        write_env_line PUBLIC_SITE_URL "$site_url"
        write_env_line UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS "$require_local_static"
        write_env_line RUST_LOG "$rust_log"
    } > "$temporary"
    chown root:"$SERVICE_GROUP" "$temporary"
    chmod 0640 "$temporary"
    mv -f "$temporary" "$ENV_FILE"
    trap - EXIT HUP INT TERM
    note "wrote $ENV_FILE"
    note "remove UNION_BOOTSTRAP_PASSWORD after the first administrator is created"
}

build_projects() {
    project_root=$1
    check_commands

    run_in "$project_root" cargo test --manifest-path union/source/Cargo.toml --all-targets --locked
    run_in "$project_root" cargo build --manifest-path union/source/Cargo.toml --release --locked
    run_in "$project_root" cargo test --manifest-path ram/source/Cargo.toml --all-targets --locked
    run_in "$project_root" cargo build --manifest-path ram/source/Cargo.toml --release --locked

    run_in "$project_root/back/source" npm ci
    run_in "$project_root/back/source" npm run build
    run_in "$project_root/blog/source" npm ci
    run_in "$project_root/blog/source" npm run build
}

ensure_service_account() {
    require_command getent
    if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
        run groupadd --system "$SERVICE_GROUP"
    fi
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        run useradd --system --gid "$SERVICE_GROUP" --home-dir "$DEPLOY_ROOT" --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
}

stage_source() {
    run install -d -o root -g root -m 0755 "$DEPLOY_ROOT"
    if [ "$SOURCE_ROOT" = "$DEPLOY_ROOT" ]; then
        note "source already resides at $DEPLOY_ROOT"
        return 0
    fi
    if [ "$DRY_RUN" -eq 1 ]; then
        note "dry-run: stage clean source tree from $SOURCE_ROOT to $DEPLOY_ROOT"
        return 0
    fi
    tar \
        --exclude='./.git' \
        --exclude='./.agents' \
        --exclude='./.codex' \
        --exclude='*/data/*' \
        --exclude='*/target' \
        --exclude='*/node_modules' \
        --exclude='*/dist' \
        --exclude='*/dist.next' \
        --exclude='*/dist.previous' \
        --exclude='*/.astro' \
        -C "$SOURCE_ROOT" -cf - . | tar -C "$DEPLOY_ROOT" -xf -
}

render_service_files() {
    require_command sed
    require_command install
    temporary_dir=$(mktemp -d)
    trap 'rm -rf "$temporary_dir"' EXIT HUP INT TERM

    sed \
        -e "s#/opt/union#$DEPLOY_ROOT#g" \
        -e "s#/etc/union/union.env#$ENV_FILE#g" \
        -e "s/^User=union$/User=$SERVICE_USER/" \
        -e "s/^Group=union$/Group=$SERVICE_GROUP/" \
        "$SOURCE_ROOT/config/systemd/union.service" > "$temporary_dir/union.service"
    sed \
        -e "s#/opt/union#$DEPLOY_ROOT#g" \
        -e "s/ union union / $SERVICE_USER $SERVICE_GROUP /g" \
        "$SOURCE_ROOT/config/tmpfiles-union.conf" > "$temporary_dir/union.conf"
    sed \
        -e "s#/opt/union#$DEPLOY_ROOT#g" \
        -e "s/su union union/su $SERVICE_USER $SERVICE_GROUP/" \
        -e "s/create 0600 union union/create 0600 $SERVICE_USER $SERVICE_GROUP/" \
        "$SOURCE_ROOT/config/logrotate-union" > "$temporary_dir/union.logrotate"

    run install -o root -g root -m 0644 "$temporary_dir/union.service" "$SYSTEMD_DIR/union.service"
    run install -o root -g root -m 0644 "$temporary_dir/union.conf" "$TMPFILES_DIR/union.conf"
    run install -o root -g root -m 0644 "$temporary_dir/union.logrotate" "$LOGROTATE_DIR/union"
    if [ "$DRY_RUN" -eq 0 ]; then
        rm -rf "$temporary_dir"
        trap - EXIT HUP INT TERM
    fi
}

install_project() {
    require_root
    validate_settings
    check_layout
    check_commands
    require_command systemctl
    require_command systemd-analyze
    require_command systemd-tmpfiles
    if [ "$DRY_RUN" -eq 0 ]; then
        [ -f "$ENV_FILE" ] || die "$ENV_FILE does not exist; run configure first"
    else
        note "dry-run: require protected environment file at $ENV_FILE"
    fi

    ensure_service_account
    stage_source
    build_projects "$DEPLOY_ROOT"

    run install -d -o root -g root -m 0755 "$DEPLOY_ROOT/bin"
    run install -o root -g root -m 0755 "$DEPLOY_ROOT/union/source/target/release/union" "$DEPLOY_ROOT/bin/union"
    run install -o root -g root -m 0755 "$DEPLOY_ROOT/ram/source/target/release/ram" "$DEPLOY_ROOT/bin/ram"

    run chown root:"$SERVICE_GROUP" "$DEPLOY_ROOT/blog/source"
    run chmod 1775 "$DEPLOY_ROOT/blog/source"
    run chmod -R u+rwX,go+rX-w "$DEPLOY_ROOT/back/source/dist" "$DEPLOY_ROOT/blog/source/dist"
    run install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0700 \
        "$DEPLOY_ROOT/back/data" \
        "$DEPLOY_ROOT/blog/data" \
        "$DEPLOY_ROOT/blog/data/logs" \
        "$DEPLOY_ROOT/ram/data" \
        "$DEPLOY_ROOT/ram/data/logs" \
        "$DEPLOY_ROOT/union/data"
    run chown -R "$SERVICE_USER:$SERVICE_GROUP" "$DEPLOY_ROOT/blog/source/dist"
    run install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0755 \
        "$DEPLOY_ROOT/blog/source/dist.next" \
        "$DEPLOY_ROOT/blog/source/dist.previous"
    run install -d -o "$SERVICE_USER" -g "$SERVICE_GROUP" -m 0700 \
        "$DEPLOY_ROOT/blog/source/.astro" \
        "$DEPLOY_ROOT/blog/source/node_modules/.vite"
    run chown -R "$SERVICE_USER:$SERVICE_GROUP" "$DEPLOY_ROOT/blog/source/.astro"
    run chown -R "$SERVICE_USER:$SERVICE_GROUP" "$DEPLOY_ROOT/blog/source/node_modules/.vite"

    render_service_files
    run systemd-tmpfiles --create "$TMPFILES_DIR/union.conf"
    run systemd-analyze verify "$SYSTEMD_DIR/union.service"
    run systemctl daemon-reload
    run rm -rf \
        "$DEPLOY_ROOT/union/source/target" \
        "$DEPLOY_ROOT/ram/source/target" \
        "$DEPLOY_ROOT/back/source/node_modules"

    if [ "$START_AFTER_INSTALL" -eq 1 ]; then
        start_service
    else
        note "installation complete; run '$0 start' after configuring Caddy and PostgreSQL"
    fi
}

start_service() {
    require_root
    run systemctl enable --now union.service
    note "union.service enabled and started"
}

verify_service() {
    require_command systemctl
    require_command curl
    systemctl --no-pager --full status union.service
    curl --fail --silent --show-error http://127.0.0.1:8080/api/health
    printf '\n'
    curl --fail --silent --show-error http://127.0.0.1:8080/api/ready
    printf '\n'
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --start) START_AFTER_INSTALL=1 ;;
        -h|--help) usage; exit 0 ;;
        check|configure|build|install|start|verify|all)
            [ -z "$COMMAND" ] || die "only one command may be specified"
            COMMAND=$1
            ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

[ -n "$COMMAND" ] || { usage; exit 1; }

case "$COMMAND" in
    check) check_all ;;
    configure) validate_settings; configure_environment ;;
    build) validate_settings; check_layout; build_projects "$SOURCE_ROOT" ;;
    install) install_project ;;
    start) validate_settings; start_service ;;
    verify) validate_settings; verify_service ;;
    all)
        check_all
        configure_environment
        install_project
        ;;
esac
