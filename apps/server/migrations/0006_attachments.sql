-- The files attached to a thread's messages. The server keeps a file for as long as a thread
-- has it.
CREATE TABLE attachments (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    PRIMARY KEY (thread_id, path)
);

CREATE INDEX attachments_by_path ON attachments(path);

INSERT OR IGNORE INTO attachments (thread_id, path)
SELECT items.thread_id, attachment.value
FROM items, json_each(items.payload, '$.attachments') AS attachment
WHERE json_valid(items.payload) AND json_extract(items.payload, '$.type') = 'user';
