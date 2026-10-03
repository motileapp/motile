-- A thread that works in a git worktree of its own: the branch made for it and the branch that
-- one started from. The worktree's folder is the thread's cwd.
ALTER TABLE threads ADD COLUMN worktree_branch TEXT;
ALTER TABLE threads ADD COLUMN worktree_base TEXT;
-- The shell script that runs in every new worktree of the project.
ALTER TABLE projects ADD COLUMN setup TEXT;
