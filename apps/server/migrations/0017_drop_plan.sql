-- Plan mode is gone: the agent plans when a prompt asks it to.
ALTER TABLE threads DROP COLUMN plan;
