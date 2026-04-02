CREATE TABLE IF NOT EXISTS solve_artifacts (
    spot_id           BIGINT PRIMARY KEY REFERENCES spots(id) ON DELETE CASCADE,
    artifact_version  INTEGER NOT NULL,
    index_path        TEXT NOT NULL,
    data_path         TEXT NOT NULL,
    node_count        INTEGER NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
