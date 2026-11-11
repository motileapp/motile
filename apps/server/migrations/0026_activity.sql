-- What the user and the agents did, for the usage view's Activity tab: the clients that connected,
-- every turn once it ended, and every feature used. All three go into the change log, as the usage
-- does, and stay when their thread is deleted.
CREATE TABLE clients (
    device TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    system TEXT NOT NULL,
    form TEXT NOT NULL,
    version TEXT NOT NULL,
    seen_at REAL NOT NULL,
    change INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX clients_by_change ON clients(change);

CREATE TABLE turns (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    agent TEXT NOT NULL,
    agent_account TEXT NOT NULL,
    model TEXT NOT NULL,
    effort TEXT NOT NULL,
    access TEXT NOT NULL,
    starter TEXT NOT NULL,
    device TEXT,
    started_at REAL NOT NULL,
    ended_at REAL NOT NULL,
    outcome TEXT NOT NULL,
    waited_secs REAL NOT NULL,
    asks INTEGER NOT NULL,
    steers INTEGER NOT NULL,
    tools INTEGER NOT NULL,
    agents INTEGER NOT NULL,
    subagents INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    reasoning_tokens INTEGER NOT NULL,
    fast INTEGER,
    change INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX turns_by_time ON turns(ended_at);
CREATE INDEX turns_by_change ON turns(change);

CREATE TABLE actions (
    at REAL NOT NULL,
    actor TEXT NOT NULL,
    device TEXT,
    feature TEXT NOT NULL,
    variant TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    path TEXT,
    rtt_ms INTEGER,
    change INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX actions_by_time ON actions(at);
CREATE INDEX actions_by_change ON actions(change);

CREATE TRIGGER clients_inserted AFTER INSERT ON clients BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE clients SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER clients_updated AFTER UPDATE ON clients WHEN NEW.change = OLD.change BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE clients SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER turns_inserted AFTER INSERT ON turns BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE turns SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;
CREATE TRIGGER actions_inserted AFTER INSERT ON actions BEGIN
    UPDATE sync SET change = change + 1;
    UPDATE actions SET change = (SELECT change FROM sync) WHERE rowid = NEW.rowid;
END;

CREATE TABLE copied_clients (
    origin TEXT NOT NULL,
    device TEXT NOT NULL,
    change INTEGER NOT NULL,
    kind TEXT NOT NULL,
    system TEXT NOT NULL,
    form TEXT NOT NULL,
    version TEXT NOT NULL,
    seen_at REAL NOT NULL,
    PRIMARY KEY (origin, device)
);

CREATE TABLE copied_turns (
    origin TEXT NOT NULL,
    change INTEGER NOT NULL,
    id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    agent TEXT NOT NULL,
    agent_account TEXT NOT NULL,
    model TEXT NOT NULL,
    effort TEXT NOT NULL,
    access TEXT NOT NULL,
    starter TEXT NOT NULL,
    device TEXT,
    started_at REAL NOT NULL,
    ended_at REAL NOT NULL,
    outcome TEXT NOT NULL,
    waited_secs REAL NOT NULL,
    asks INTEGER NOT NULL,
    steers INTEGER NOT NULL,
    tools INTEGER NOT NULL,
    agents INTEGER NOT NULL,
    subagents INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    reasoning_tokens INTEGER NOT NULL,
    fast INTEGER,
    PRIMARY KEY (origin, id)
);
CREATE INDEX copied_turns_by_time ON copied_turns(origin, ended_at);

CREATE TABLE copied_actions (
    origin TEXT NOT NULL,
    change INTEGER NOT NULL,
    at REAL NOT NULL,
    actor TEXT NOT NULL,
    device TEXT,
    feature TEXT NOT NULL,
    variant TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    path TEXT,
    rtt_ms INTEGER,
    PRIMARY KEY (origin, change)
);
CREATE INDEX copied_actions_by_time ON copied_actions(origin, at);
