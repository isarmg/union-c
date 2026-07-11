-- UnionC baseline: authentication audit, runtime settings, and Sunshine hosts only.

CREATE OR REPLACE FUNCTION unionc_valid_host_address(value TEXT)
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

CREATE OR REPLACE FUNCTION unionc_valid_json_object(value TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN jsonb_typeof(value::jsonb) = 'object';
EXCEPTION WHEN others THEN
    RETURN FALSE;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT;

CREATE TABLE IF NOT EXISTS settings (
    setting_key VARCHAR(255) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id BIGSERIAL PRIMARY KEY,
    action VARCHAR(128) NOT NULL,
    target VARCHAR(128) NOT NULL,
    detail TEXT,
    actor TEXT NOT NULL DEFAULT 'system',
    request_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at);

CREATE TABLE IF NOT EXISTS external_hosts (
    kind VARCHAR(32) NOT NULL CHECK (kind = 'sunshine'),
    host_id VARCHAR(255) NOT NULL CHECK (length(trim(host_id)) > 0),
    address TEXT NOT NULL CHECK (unionc_valid_host_address(address)),
    config TEXT NOT NULL CHECK (unionc_valid_json_object(config)),
    secret TEXT CHECK (secret IS NULL OR length(secret) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (kind, host_id)
);
