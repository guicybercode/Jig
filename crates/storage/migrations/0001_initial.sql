CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    path            TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL,
    last_opened_at  TEXT NOT NULL
);

CREATE TABLE agents (
    id              TEXT PRIMARY KEY,
    source          TEXT NOT NULL CHECK (source IN ('built_in', 'custom')),
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    executable      TEXT NOT NULL CHECK (length(trim(executable)) > 0),
    args_json       TEXT NOT NULL DEFAULT '[]',
    env_json        TEXT NOT NULL DEFAULT '{}',
    enabled         INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE sessions (
    id                  TEXT PRIMARY KEY,
    project_id          TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    agent_id            TEXT NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    name                TEXT NOT NULL CHECK (length(trim(name)) > 0),
    cwd                 TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (
                            status IN ('starting','running','idle','exited','failed','unknown')
                        ),
    runtime_pid         INTEGER,
    daemon_instance_id  TEXT,
    exit_code           INTEGER,
    error_code          TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    last_activity_at    TEXT
);

CREATE TABLE worktrees (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    session_id      TEXT UNIQUE REFERENCES sessions(id) ON DELETE SET NULL,
    path            TEXT NOT NULL UNIQUE,
    branch          TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (
                        state IN ('creating','active','remove_pending','orphaned')
                    ),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(project_id, branch)
);

CREATE TABLE settings (
    key             TEXT PRIMARY KEY,
    value_json      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX sessions_by_project_updated
    ON sessions(project_id, updated_at DESC);
CREATE INDEX sessions_by_status ON sessions(status);
CREATE INDEX worktrees_by_project ON worktrees(project_id);
