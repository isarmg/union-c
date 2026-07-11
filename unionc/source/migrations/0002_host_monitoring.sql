-- Read-only host monitoring. No command or control data is stored here.

CREATE TABLE monitored_hosts (
    host_id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL CHECK (length(trim(name)) > 0),
    os VARCHAR(64) NOT NULL CHECK (length(trim(os)) > 0),
    os_version TEXT,
    kernel_version TEXT,
    arch VARCHAR(64) NOT NULL CHECK (length(trim(arch)) > 0),
    agent_version VARCHAR(128) NOT NULL CHECK (length(trim(agent_version)) > 0),
    enrollment_secret_hash CHAR(64) NOT NULL
        CHECK (enrollment_secret_hash ~ '^[0-9a-f]{64}$'),
    agent_token_hash CHAR(64) NOT NULL UNIQUE
        CHECK (agent_token_hash ~ '^[0-9a-f]{64}$'),
    capabilities JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(capabilities) = 'array'),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    latest_report_id UUID,
    latest_collected_at TIMESTAMPTZ,
    latest_interval_seconds DOUBLE PRECISION,
    latest_report JSONB
        CHECK (latest_report IS NULL OR jsonb_typeof(latest_report) = 'object')
);

CREATE INDEX idx_monitored_hosts_last_seen_at
    ON monitored_hosts(last_seen_at DESC);

CREATE TABLE agent_metric_reports (
    report_id UUID PRIMARY KEY,
    host_id UUID NOT NULL REFERENCES monitored_hosts(host_id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    collected_at TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    interval_seconds DOUBLE PRECISION NOT NULL
        CHECK (interval_seconds > 0 AND interval_seconds <= 3600),
    payload JSONB NOT NULL CHECK (jsonb_typeof(payload) = 'object')
);

CREATE INDEX idx_agent_metric_reports_host_collected_at
    ON agent_metric_reports(host_id, collected_at DESC, report_id);

CREATE INDEX idx_agent_metric_reports_received_at
    ON agent_metric_reports(received_at);
