-- Additive recovery metadata. Existing rows stay readable and are backfilled
-- in place; foreign keys remain intact because tables are not rebuilt.

ALTER TABLE projects ADD COLUMN repository_root TEXT;
ALTER TABLE projects ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

UPDATE projects
SET updated_at = created_at
WHERE updated_at = '';

ALTER TABLE sessions ADD COLUMN branch TEXT;
ALTER TABLE sessions ADD COLUMN worktree_path TEXT;
ALTER TABLE sessions ADD COLUMN started_at TEXT;
ALTER TABLE sessions ADD COLUMN exited_at TEXT;

CREATE INDEX IF NOT EXISTS projects_by_last_opened
    ON projects(last_opened_at DESC);
CREATE INDEX IF NOT EXISTS agents_by_source_enabled
    ON agents(source, enabled);
CREATE INDEX IF NOT EXISTS sessions_by_agent
    ON sessions(agent_id);
CREATE INDEX IF NOT EXISTS custom_agents_by_updated
    ON agents(updated_at DESC)
    WHERE source = 'custom';
