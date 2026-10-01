CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    created_at REAL NOT NULL
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    title_source TEXT NOT NULL,
    project_id TEXT NOT NULL,
    cwd TEXT NOT NULL,
    agent TEXT NOT NULL,
    model TEXT,
    effort TEXT,
    access TEXT NOT NULL,
    plan INTEGER NOT NULL DEFAULT 0,
    session_id TEXT,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    done_at REAL,
    undone_at REAL,
    needs_approval INTEGER NOT NULL DEFAULT 0,
    turn_ended_at REAL
);

CREATE TABLE items (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    rev INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (thread_id, id)
);

CREATE INDEX items_by_seq ON items(thread_id, seq);
CREATE INDEX items_by_rev ON items(thread_id, rev);
