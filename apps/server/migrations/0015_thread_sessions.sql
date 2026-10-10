-- A thread's native session with each agent and sessions folder it has run in.
CREATE TABLE thread_sessions (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    sessions_folder TEXT NOT NULL,
    session_id TEXT,
    model TEXT,
    seen_through_seq INTEGER NOT NULL DEFAULT 0,
    pending_through_seq INTEGER,
    context_used INTEGER,
    context_window INTEGER,
    last_turn_at REAL,
    PRIMARY KEY (thread_id, agent, sessions_folder)
);
