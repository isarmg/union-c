-- Backfill RAM instance TLS columns for databases that applied an older baseline
-- before schema checksums were introduced.

ALTER TABLE ram_instances
    ADD COLUMN IF NOT EXISTS use_tls BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS verify_tls BOOLEAN NOT NULL DEFAULT TRUE;
