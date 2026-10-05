-- What the tokens were spent on: a thread's turn, or writing a title, a branch's name, a commit
-- message or a pull request, which has no thread when it was asked for outside one.
ALTER TABLE usage ADD COLUMN purpose TEXT NOT NULL DEFAULT 'turn';
