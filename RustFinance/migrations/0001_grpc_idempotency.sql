CREATE TABLE IF NOT EXISTS grpc_idempotency_keys (
    idempotency_key UUID PRIMARY KEY,
    operation TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('Started', 'Completed', 'Failed')),
    response_json JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_grpc_idempotency_keys_status
    ON grpc_idempotency_keys (status);
