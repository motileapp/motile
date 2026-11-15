-- A started agent says what its thread has spent.
CREATE INDEX usage_by_thread ON usage(thread_id);
