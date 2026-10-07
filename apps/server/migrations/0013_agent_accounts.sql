-- The account of its agent that a thread works with, and that spent what was used. Until now
-- each agent had one, the default, which has the agent's name.
ALTER TABLE threads ADD COLUMN agent_account TEXT NOT NULL DEFAULT '';
UPDATE threads SET agent_account = agent;
ALTER TABLE usage ADD COLUMN agent_account TEXT NOT NULL DEFAULT '';
UPDATE usage SET agent_account = agent;
