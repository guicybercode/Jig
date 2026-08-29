import type { Project } from "../../../ipc/types";
import { Icon } from "../../components/Icon";

interface ProjectSidebarProps {
  readonly projects: readonly Project[];
  readonly selectedProjectId?: string;
  readonly canManageProjects: boolean;
  readonly onSelectProject: (projectId: string) => void;
  readonly onAddProject: () => void;
  readonly onRenameProject: (projectId: string) => void;
  readonly onRemoveProject: (projectId: string) => void;
  readonly onOpenSettings: () => void;
  readonly onOpenDiagnostics: () => void;
}

/** Renders recent project navigation and metadata-only project actions. */
export function ProjectSidebar({
  projects,
  selectedProjectId,
  canManageProjects,
  onSelectProject,
  onAddProject,
  onRenameProject,
  onRemoveProject,
  onOpenSettings,
  onOpenDiagnostics,
}: ProjectSidebarProps) {
  const orderedProjects = [...projects].sort(
    (left, right) => right.lastOpenedAtMs - left.lastOpenedAtMs,
  );

  return (
    <aside className="project-rail" aria-label="Projects">
      <div className="pane-heading">
        <div>
          <span className="pane-heading__label">Projects</span>
          <span className="pane-heading__count" aria-label={`${projects.length} projects`}>
            {projects.length}
          </span>
        </div>
        <button
          className="icon-button"
          type="button"
          aria-label="Add Project"
          title="Add Project"
          disabled={!canManageProjects}
          onClick={onAddProject}
        >
          <Icon name="plus" />
        </button>
      </div>

      <nav className="pane-scroll" aria-label="Recent projects">
        {orderedProjects.length === 0 ? (
          <div className="nav-empty">
            <Icon name="folder" />
            <p>No repositories added.</p>
            <button className="button button--secondary button--full" type="button" disabled={!canManageProjects} onClick={onAddProject}>
              <Icon name="plus" />
              Add Project
            </button>
          </div>
        ) : (
          <ul className="navigation-list">
            {orderedProjects.map((project) => {
              const isSelected = project.id === selectedProjectId;
              const isUnavailable =
                project.availability === "missing" ||
                project.availability === "not_repository";
              return (
                <li className="navigation-item" key={project.id}>
                  <button
                    className="navigation-row"
                    type="button"
                    aria-current={isSelected ? "page" : undefined}
                    onClick={() => onSelectProject(project.id)}
                  >
                    <span className="navigation-row__icon" aria-hidden="true">
                      <Icon name="repository" />
                    </span>
                    <span className="navigation-row__content">
                      <span className="navigation-row__title">{project.name}</span>
                      <span className="navigation-row__meta">
                        {isUnavailable
                          ? "Moved or unavailable"
                          : project.currentBranch ?? "Branch unavailable"}
                      </span>
                    </span>
                  </button>
                  <div className="navigation-item__actions">
                    <button
                      className="icon-button icon-button--small"
                      type="button"
                      aria-label={`Rename ${project.name}`}
                      title="Rename project"
                      disabled={!canManageProjects}
                      onClick={() => onRenameProject(project.id)}
                    >
                      <Icon name="pencil" />
                    </button>
                    <button
                      className="icon-button icon-button--small"
                      type="button"
                      aria-label={`Remove ${project.name} from Jig`}
                      title="Remove project metadata"
                      disabled={!canManageProjects}
                      onClick={() => onRemoveProject(project.id)}
                    >
                      <Icon name="close" />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </nav>

      <div className="pane-footer">
        <button className="pane-footer__action" type="button" onClick={onOpenSettings}>
          <Icon name="settings" />
          <span>Settings</span>
        </button>
        <button className="pane-footer__action" type="button" onClick={onOpenDiagnostics}>
          <Icon name="diagnostics" />
          <span>Diagnostics</span>
        </button>
      </div>
    </aside>
  );
}
