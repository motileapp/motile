-- The images and videos a thread's items show. The host keeps a file for as long as a thread
-- shows it.
CREATE TABLE media (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    PRIMARY KEY (thread_id, id)
);

CREATE INDEX media_by_id ON media(id);
