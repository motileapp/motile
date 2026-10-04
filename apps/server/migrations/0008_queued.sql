-- The messages that wait for a thread's agent, in the order they are given to it.
CREATE TABLE queued (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    position INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (thread_id, id)
);
