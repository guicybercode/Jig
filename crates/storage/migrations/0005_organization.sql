-- User organization metadata never alters daemon-owned process state.
CREATE TABLE project_organization (
    project_id      TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    pinned          INTEGER NOT NULL CHECK (typeof(pinned) = 'integer' AND pinned IN (0, 1)),
    archived        INTEGER NOT NULL CHECK (typeof(archived) = 'integer' AND archived IN (0, 1)),
    revision        INTEGER NOT NULL CHECK (
                        typeof(revision) = 'integer' AND revision BETWEEN 1 AND 9007199254740991
                    ),
    updated_at_ms   INTEGER NOT NULL CHECK (
                        typeof(updated_at_ms) = 'integer' AND updated_at_ms BETWEEN 0 AND 9007199254740991
                    )
);

CREATE TABLE session_organization (
    session_id      TEXT PRIMARY KEY NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    pinned          INTEGER NOT NULL CHECK (typeof(pinned) = 'integer' AND pinned IN (0, 1)),
    archived        INTEGER NOT NULL CHECK (typeof(archived) = 'integer' AND archived IN (0, 1)),
    workflow        TEXT NOT NULL CHECK (workflow IN ('backlog', 'in_progress', 'in_review', 'blocked', 'done')),
    revision        INTEGER NOT NULL CHECK (
                        typeof(revision) = 'integer' AND revision BETWEEN 1 AND 9007199254740991
                    ),
    updated_at_ms   INTEGER NOT NULL CHECK (
                        typeof(updated_at_ms) = 'integer' AND updated_at_ms BETWEEN 0 AND 9007199254740991
                    )
);
