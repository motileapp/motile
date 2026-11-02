CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    agent TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT,
    plan TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL,
    tokens BLOB NOT NULL,
    windows TEXT NOT NULL DEFAULT '[]',
    read_at REAL,
    reset_credits INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL
);

CREATE TABLE settings (
    name TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE readings (
    account TEXT NOT NULL,
    at REAL NOT NULL,
    windows TEXT NOT NULL
);
CREATE INDEX readings_by_account ON readings (account, at);

CREATE TABLE log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at REAL NOT NULL,
    finished_at REAL NOT NULL,
    server TEXT NOT NULL,
    thread TEXT NOT NULL,
    agent TEXT NOT NULL,
    account TEXT,
    model TEXT,
    path TEXT NOT NULL,
    status INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    cost REAL NOT NULL,
    retries INTEGER NOT NULL,
    request_bytes INTEGER NOT NULL,
    response_bytes INTEGER NOT NULL,
    error TEXT,
    captured INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX log_by_time ON log (at);
CREATE INDEX log_by_account ON log (account, finished_at);
CREATE INDEX log_errors ON log (id) WHERE error IS NOT NULL;

CREATE TABLE captures (
    entry INTEGER PRIMARY KEY,
    at REAL NOT NULL,
    size INTEGER NOT NULL,
    request_headers TEXT NOT NULL,
    request_body BLOB NOT NULL,
    response_headers TEXT NOT NULL,
    response_body BLOB NOT NULL
);

CREATE TABLE usage (
    hour REAL NOT NULL,
    account TEXT NOT NULL,
    model TEXT NOT NULL,
    requests INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cache_read_tokens INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    cost REAL NOT NULL,
    PRIMARY KEY (hour, account, model)
);
