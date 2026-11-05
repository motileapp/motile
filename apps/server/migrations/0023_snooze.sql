-- A thread can be snoozed until a time, and remembers when it last woke by itself.
ALTER TABLE threads ADD COLUMN snoozed_until REAL;
ALTER TABLE threads ADD COLUMN woke_at REAL;
