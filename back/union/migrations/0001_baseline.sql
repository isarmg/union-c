-- Union baseline schema. Applied once and tracked in schema_migrations.

CREATE OR REPLACE FUNCTION union_valid_host_address(value TEXT)
        RETURNS BOOLEAN AS $$
        DECLARE candidate TEXT := trim(both '[]' from trim(value));
        BEGIN
            IF candidate = '' THEN RETURN FALSE; END IF;
            BEGIN
                PERFORM candidate::inet;
                RETURN TRUE;
            EXCEPTION WHEN invalid_text_representation THEN
                RETURN candidate ~ '^(?=.{1,253}\.?$)([A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)*[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.?$';
            END;
        END;
        $$ LANGUAGE plpgsql IMMUTABLE STRICT;

CREATE TABLE IF NOT EXISTS managed_host_addresses (
            kind       VARCHAR(32) NOT NULL,
            host_id    VARCHAR(255) NOT NULL,
            address    TEXT NOT NULL CHECK (union_valid_host_address(address)),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (kind, host_id)
        );

CREATE TABLE IF NOT EXISTS ram_instances (
            id            VARCHAR(64) PRIMARY KEY,
            name          VARCHAR(255) NOT NULL CHECK (length(trim(name)) > 0),
            bind_address  TEXT NOT NULL CHECK (union_valid_host_address(bind_address)),
            port          INTEGER NOT NULL UNIQUE CHECK (port BETWEEN 1 AND 65535),
            serve_path    TEXT NOT NULL CHECK (length(trim(serve_path)) > 0),
            desired_state VARCHAR(32) NOT NULL DEFAULT 'stopped' CHECK (desired_state IN ('running', 'stopped')),
            created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

ALTER TABLE ram_instances
            ADD COLUMN IF NOT EXISTS use_tls BOOLEAN NOT NULL DEFAULT FALSE,
            ADD COLUMN IF NOT EXISTS verify_tls BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE ram_instances DROP CONSTRAINT IF EXISTS ram_instances_port_key;

CREATE TABLE IF NOT EXISTS services (
            name          VARCHAR(64) PRIMARY KEY,
            kind          VARCHAR(64) NOT NULL,
            desired_state VARCHAR(32) NOT NULL DEFAULT 'stopped',
            updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

CREATE TABLE IF NOT EXISTS service_events (
            id           BIGSERIAL PRIMARY KEY,
            service_name VARCHAR(64) NOT NULL,
            action       VARCHAR(64) NOT NULL,
            message      TEXT,
            created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

CREATE TABLE IF NOT EXISTS jobs (
            id          VARCHAR(64) PRIMARY KEY,
            kind        VARCHAR(64) NOT NULL,
            status      VARCHAR(32) NOT NULL,
            exit_code   INT,
            duration_ms BIGINT,
            log_path    TEXT,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            finished_at TIMESTAMPTZ
        );

CREATE TABLE IF NOT EXISTS job_logs (
            id         BIGSERIAL PRIMARY KEY,
            job_id     VARCHAR(64) NOT NULL,
            line       TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CONSTRAINT fk_job_logs_job FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
        );

CREATE TABLE IF NOT EXISTS blog_posts (
            id           VARCHAR(255) PRIMARY KEY,
            relative_path TEXT NOT NULL,
            extension    VARCHAR(16) NOT NULL DEFAULT 'md',
            title        TEXT NOT NULL,
            description  TEXT,
            content      TEXT,
            draft        BOOLEAN NOT NULL DEFAULT TRUE,
            featured     BOOLEAN NOT NULL DEFAULT FALSE,
            pub_date     VARCHAR(32),
            updated_date VARCHAR(32),
            author       VARCHAR(255),
            category     VARCHAR(255),
            series       VARCHAR(255),
            hero_image   TEXT,
            created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

CREATE TABLE IF NOT EXISTS blog_post_tags (
            post_id    VARCHAR(255) NOT NULL,
            tag        VARCHAR(255) NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (post_id, tag),
            CONSTRAINT fk_blog_post_tags_post
                FOREIGN KEY (post_id) REFERENCES blog_posts(id)
                ON DELETE CASCADE
        );

CREATE TABLE IF NOT EXISTS blog_taxonomy (
            kind       VARCHAR(32) NOT NULL,
            name       VARCHAR(255) NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (kind, name)
        );

CREATE TABLE IF NOT EXISTS blog_category_tags (
            category   VARCHAR(255) NOT NULL,
            tag        VARCHAR(255) NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (category, tag)
        );

CREATE TABLE IF NOT EXISTS settings (
            setting_key VARCHAR(255) PRIMARY KEY,
            value       TEXT NOT NULL,
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

CREATE TABLE IF NOT EXISTS audit_logs (
            id         BIGSERIAL PRIMARY KEY,
            action     VARCHAR(128) NOT NULL,
            target     VARCHAR(128) NOT NULL,
            detail     TEXT,
            actor      TEXT NOT NULL DEFAULT 'system',
            request_id TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

CREATE TABLE IF NOT EXISTS service_accounts (
            id             BIGSERIAL PRIMARY KEY,
            service_name   VARCHAR(64) NOT NULL,
            account_key    VARCHAR(255) NOT NULL,
            username       VARCHAR(255),
            password_secret TEXT,
            is_anonymous   BOOLEAN NOT NULL DEFAULT FALSE,
            is_management  BOOLEAN NOT NULL DEFAULT FALSE,
            enabled        BOOLEAN NOT NULL DEFAULT TRUE,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE (service_name, account_key)
        );

CREATE TABLE IF NOT EXISTS service_account_permissions (
            id            BIGSERIAL PRIMARY KEY,
            account_id    BIGINT NOT NULL,
            resource_path TEXT NOT NULL,
            permission    VARCHAR(32) NOT NULL,
            created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CONSTRAINT fk_service_account_permissions_account
                FOREIGN KEY (account_id) REFERENCES service_accounts(id)
                ON DELETE CASCADE
        );

DROP TABLE IF EXISTS sessions;

DROP TABLE IF EXISTS users;

CREATE INDEX IF NOT EXISTS idx_blog_post_tags_tag ON blog_post_tags (tag);

CREATE INDEX IF NOT EXISTS idx_blog_category_tags_tag ON blog_category_tags (tag);

CREATE INDEX IF NOT EXISTS idx_service_accounts_service ON service_accounts (service_name);

CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs (created_at);

CREATE INDEX IF NOT EXISTS idx_service_events_created_at ON service_events (created_at);

CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs (created_at);

INSERT INTO services (name, kind, desired_state) VALUES
    ('ram', 'file-service', 'stopped'),
    ('sunshine', 'streaming-host', 'stopped'),
    ('moonlight', 'streaming-client', 'stopped'),
    ('blog', 'static-site', 'stopped')
ON CONFLICT (name) DO NOTHING;
