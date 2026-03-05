CREATE TABLE certificates (
    id TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL,
    provider TEXT NOT NULL,
    names TEXT NOT NULL,
    cert_pem TEXT,
    key_pem TEXT,
    ca_pem TEXT,
    chain_pem TEXT,
    expires_at INTEGER NOT NULL DEFAULT 0,
    prefer_renew_before INTEGER,
    prefer_renew_after INTEGER,
    requested_at INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
);

CREATE INDEX idx_certificates_expires_at ON certificates(expires_at);
CREATE INDEX idx_certificates_requested_at ON certificates(requested_at);
CREATE INDEX idx_certificates_source ON certificates(source);
