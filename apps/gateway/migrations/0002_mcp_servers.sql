CREATE TABLE mcp_servers (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    login BLOB NOT NULL,
    status TEXT NOT NULL,
    tools TEXT NOT NULL DEFAULT '[]',
    off TEXT NOT NULL DEFAULT '[]',
    icon TEXT,
    created_at REAL NOT NULL
);

CREATE TABLE calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at REAL NOT NULL,
    server TEXT,
    thread TEXT,
    mcp_server TEXT NOT NULL,
    tool TEXT NOT NULL,
    latency_ms INTEGER NOT NULL,
    error TEXT,
    arguments TEXT NOT NULL,
    result TEXT NOT NULL
);
CREATE INDEX calls_by_time ON calls (at);
CREATE INDEX calls_errors ON calls (id) WHERE error IS NOT NULL;
