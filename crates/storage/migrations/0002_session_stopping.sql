CREATE TABLE sessions_new (
    id                  TEXT PRIMARY KEY,
    project_id          TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    agent_id            TEXT NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    name                TEXT NOT NULL CHECK (length(trim(name)) > 0),
    cwd                 TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (
                            status IN ('starting','running','idle','stopping','exited','failed','unknown')
                        ),
    runtime_pid         INTEGER,
    daemon_instance_id  TEXT,
    exit_code           INTEGER,
    error_code          TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    last_activity_at    TEXT
);

INSERT INTO sessions_new (
    id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
    exit_code, error_code, created_at, updated_at, last_activity_at
)
SELECT
    id, project_id, agent_id, name, cwd, status, runtime_pid, daemon_instance_id,
    exit_code, error_code, created_at, updated_at, last_activity_at
FROM sessions;

CREATE TABLE worktrees_new (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    session_id      TEXT UNIQUE REFERENCES sessions_new(id) ON DELETE SET NULL,
    path            TEXT NOT NULL UNIQUE,
    branch          TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (
                        state IN ('creating','active','remove_pending','orphaned')
                    ),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(project_id, branch)
);

INSERT INTO worktrees_new (
    id, project_id, session_id, path, branch, state, created_at, updated_at
)
SELECT
    id, project_id, session_id, path, branch, state, created_at, updated_at
FROM worktrees;

DROP TABLE worktrees;
DROP TABLE sessions;

ALTER TABLE sessions_new RENAME TO sessions;
ALTER TABLE worktrees_new RENAME TO worktrees;

CREATE INDEX sessions_by_project_updated
    ON sessions(project_id, updated_at DESC);
CREATE INDEX sessions_by_status ON sessions(status);
CREATE INDEX worktrees_by_project ON worktrees(project_id);
