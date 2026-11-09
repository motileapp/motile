-- The thread whose agent started a thread, with start_agent or create_thread. The user's threads
-- have none, wherever they sit.
ALTER TABLE threads ADD COLUMN started_by TEXT;

-- The threads started before this was kept. Every thread with a parent then was an agent's.
UPDATE threads SET started_by = parent_thread WHERE parent_thread IS NOT NULL;

-- The create_thread call that made one answered with its id.
UPDATE threads SET started_by = made.by
FROM (
    SELECT thread_id AS by, json_extract(json_extract(payload, '$.call.output'), '$.id') AS id
    FROM items
    WHERE payload LIKE '%"name":"mcp__motile__create_thread"%'
      AND json_extract(payload, '$.type') = 'tool'
      AND json_extract(payload, '$.call.name') = 'mcp__motile__create_thread'
      AND json_extract(payload, '$.call.status') = 'succeeded'
      AND json_valid(json_extract(payload, '$.call.output'))
) AS made
WHERE threads.id = made.id AND threads.parent_thread IS NULL;
