CREATE TABLE IF NOT EXISTS spots (
    id          BIGSERIAL PRIMARY KEY,
    label       TEXT NOT NULL,
    board       TEXT NOT NULL,
    oop_range   TEXT NOT NULL,
    ip_range    TEXT NOT NULL,
    pot         INTEGER NOT NULL,
    stack       INTEGER NOT NULL,
    exploitability REAL NOT NULL,
    iterations  INTEGER NOT NULL,
    game_data   BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
