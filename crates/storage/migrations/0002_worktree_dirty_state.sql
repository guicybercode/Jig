ALTER TABLE worktrees
    ADD COLUMN is_dirty INTEGER NOT NULL DEFAULT 0
        CHECK (is_dirty IN (0, 1));

CREATE TRIGGER worktrees_same_project_on_insert
BEFORE INSERT ON worktrees
WHEN NEW.session_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1
    FROM sessions
    WHERE sessions.id = NEW.session_id
      AND sessions.project_id = NEW.project_id
 )
BEGIN
    SELECT RAISE(ABORT, 'worktree session must belong to its project');
END;

CREATE TRIGGER worktrees_same_project_on_update
BEFORE UPDATE OF project_id, session_id ON worktrees
WHEN NEW.session_id IS NOT NULL
 AND NOT EXISTS (
    SELECT 1
    FROM sessions
    WHERE sessions.id = NEW.session_id
      AND sessions.project_id = NEW.project_id
 )
BEGIN
    SELECT RAISE(ABORT, 'worktree session must belong to its project');
END;
