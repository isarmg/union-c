-- Tighten storage invariants and remove structures that were never consumed.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM blog_posts
        GROUP BY relative_path
        HAVING COUNT(*) > 1
    ) THEN
        RAISE EXCEPTION 'cannot add unique blog path constraint: duplicate relative_path values exist';
    END IF;

    IF EXISTS (
        SELECT 1 FROM blog_posts
        WHERE (pub_date IS NOT NULL AND pub_date !~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$')
           OR (updated_date IS NOT NULL AND updated_date !~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$')
    ) THEN
        RAISE EXCEPTION 'cannot convert blog dates: non-ISO date values exist';
    END IF;
END
$$;

ALTER TABLE blog_posts
    ALTER COLUMN pub_date TYPE DATE USING pub_date::date,
    ALTER COLUMN updated_date TYPE DATE USING updated_date::date;

CREATE UNIQUE INDEX idx_blog_posts_relative_path
    ON blog_posts (relative_path);

ALTER TABLE services
    ADD CONSTRAINT services_desired_state_check
    CHECK (desired_state IN ('running', 'stopped'));

ALTER TABLE jobs
    ADD CONSTRAINT jobs_status_check
    CHECK (status IN ('running', 'succeeded', 'failed', 'abandoned'));

ALTER TABLE blog_taxonomy
    ADD CONSTRAINT blog_taxonomy_kind_check
    CHECK (kind IN ('tag', 'category'));

ALTER TABLE service_account_permissions
    ADD CONSTRAINT service_account_permissions_permission_check
    CHECK (permission IN ('ro', 'rw'));

DELETE FROM service_account_permissions newer
USING service_account_permissions older
WHERE newer.account_id = older.account_id
  AND newer.resource_path = older.resource_path
  AND newer.id > older.id;

CREATE UNIQUE INDEX idx_service_account_permissions_account_path
    ON service_account_permissions (account_id, resource_path);

CREATE TABLE external_hosts (
    kind          VARCHAR(32) NOT NULL CHECK (kind IN ('sunshine', 'proxmox')),
    host_id       VARCHAR(255) NOT NULL,
    address       TEXT NOT NULL CHECK (union_valid_host_address(address)),
    config        TEXT NOT NULL,
    secret        TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (kind, host_id)
);

DROP TABLE IF EXISTS job_logs;
DROP TABLE IF EXISTS managed_host_addresses;
