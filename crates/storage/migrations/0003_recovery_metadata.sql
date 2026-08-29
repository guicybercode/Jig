-- Additive session history used during daemon recovery. Runtime ownership is
-- still in memory; none of these columns is a liveness signal.
ALTER TABLE sessions ADD COLUMN branch TEXT;
ALTER TABLE sessions ADD COLUMN worktree_path BLOB;
ALTER TABLE sessions ADD COLUMN started_at INTEGER;
ALTER TABLE sessions ADD COLUMN exited_at INTEGER;

CREATE INDEX IF NOT EXISTS projects_by_last_opened
    ON projects(last_opened_at DESC);
CREATE INDEX IF NOT EXISTS agents_by_source_enabled
    ON agents(source, enabled);
CREATE INDEX IF NOT EXISTS sessions_by_agent
    ON sessions(agent_id);
CREATE INDEX IF NOT EXISTS sessions_by_daemon_status
    ON sessions(daemon_instance_id, status)
    WHERE status IN ('starting', 'running', 'idle');
CREATE INDEX IF NOT EXISTS custom_agents_by_updated
    ON agents(updated_at DESC)
    WHERE source = 'custom';
