-- Explicitly saved local content. Project removal deletes scoped metadata only.
CREATE TABLE knowledge_documents (
    id              TEXT PRIMARY KEY NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('prompt', 'context')),
    project_id      TEXT REFERENCES projects(id) ON DELETE CASCADE,
    title           TEXT NOT NULL CHECK (
                        length(CAST(title AS BLOB)) BETWEEN 1 AND 256
                        AND instr(title, char(0)) = 0
                    ),
    body            TEXT NOT NULL CHECK (
                        length(CAST(body AS BLOB)) BETWEEN 1 AND 65536
                        AND instr(body, char(0)) = 0
                    ),
    revision        INTEGER NOT NULL CHECK (
                        typeof(revision) = 'integer'
                        AND revision BETWEEN 1 AND 9007199254740991
                    ),
    created_at_ms   INTEGER NOT NULL CHECK (
                        typeof(created_at_ms) = 'integer'
                        AND created_at_ms BETWEEN 0 AND 9007199254740991
                    ),
    updated_at_ms   INTEGER NOT NULL CHECK (
                        typeof(updated_at_ms) = 'integer'
                        AND updated_at_ms BETWEEN created_at_ms AND 9007199254740991
                    )
);

CREATE INDEX knowledge_by_scope_id ON knowledge_documents(project_id, id);
CREATE INDEX knowledge_by_scope_kind_id ON knowledge_documents(project_id, kind, id);
