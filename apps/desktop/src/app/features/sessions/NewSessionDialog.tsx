import {
  useId,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type RefObject,
} from "react";

import type {
  AgentDetection,
  AgentRecord,
  ApiErrorData,
  CreateCustomAgentInput,
  CreateSessionInput,
  Project,
  Session,
} from "../../../ipc/types";
import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import { errorData, suggestBranch } from "../../utils";
import { InlineRequestError } from "../projects/ProjectDialogs";

const CUSTOM_AGENT_VALUE = "__custom_agent__";

interface NewSessionDialogProps {
  readonly open: boolean;
  readonly project: Project;
  readonly agents: readonly AgentRecord[];
  readonly agentDetections: readonly AgentDetection[];
  readonly onClose: () => void;
  readonly onCreateCustomAgent: (
    input: CreateCustomAgentInput,
  ) => Promise<AgentRecord>;
  readonly onCreate: (input: CreateSessionInput) => Promise<Session>;
}

type FormStep = "details" | "review";
type WorktreeMode = "current" | "new";
type FieldName =
  | "name"
  | "agent"
  | "customName"
  | "executable"
  | "relativeDirectory";

/** Two-step, duplicate-safe flow that sends only official session intent. */
export function NewSessionDialog({
  open,
  project,
  agents,
  agentDetections,
  onClose,
  onCreateCustomAgent,
  onCreate,
}: NewSessionDialogProps) {
  const formId = useId();
  const nameId = useId();
  const agentId = useId();
  const customNameId = useId();
  const executableId = useId();
  const customArgsId = useId();
  const relativeDirectoryId = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const agentRef = useRef<HTMLSelectElement>(null);
  const customNameRef = useRef<HTMLInputElement>(null);
  const executableRef = useRef<HTMLInputElement>(null);
  const relativeDirectoryRef = useRef<HTMLInputElement>(null);
  const inFlight = useRef(false);
  const createdCustomAgentId = useRef<string | undefined>(undefined);
  const [step, setStep] = useState<FormStep>("details");
  const [name, setName] = useState("");
  const [selectedAgentId, setSelectedAgentId] = useState(() =>
    firstSelectableAgentId(agents, agentDetections),
  );
  const [customName, setCustomName] = useState("");
  const [executable, setExecutable] = useState("");
  const [customArgs, setCustomArgs] = useState("");
  const [relativeDirectory, setRelativeDirectory] = useState("");
  const [worktreeMode, setWorktreeMode] = useState<WorktreeMode>("current");
  const [errors, setErrors] = useState<Partial<Record<FieldName, string>>>({});
  const [requestError, setRequestError] = useState<ApiErrorData>();
  const [submitting, setSubmitting] = useState(false);

  const selectedAgent = agents.find(
    (agent) => agent.id === selectedAgentId,
  );
  const selectedDetection = agentDetections.find(
    (detection) => detection.agentId === selectedAgentId,
  );
  const unavailableAgentNames = agents
    .filter((agent) =>
      agentDetections.some(
        (detection) =>
          detection.agentId === agent.id && !detection.available,
      ),
    )
    .map((agent) => agent.displayName);
  const isCustom = selectedAgentId === CUSTOM_AGENT_VALUE;
  const parsedCustomArgs = useMemo(
    () => parseArgumentLines(customArgs),
    [customArgs],
  );
  const projectRoot = project.repositoryRoot ?? project.path;
  const effectiveDirectory = joinDisplayPath(
    projectRoot,
    relativeDirectory.trim(),
  );
  const suggestedBranch = suggestBranch(name);

  function validate(): boolean {
    const next: Partial<Record<FieldName, string>> = {};
    if (!name.trim()) next.name = "Enter a session name.";
    if (!selectedAgentId) next.agent = "Choose an agent.";
    if (
      !isCustom &&
      (!selectedAgent ||
        !selectedAgent.enabled ||
        selectedDetection?.available === false)
    ) {
      next.agent = "Choose an enabled agent whose executable is available.";
    }
    if (isCustom && !customName.trim()) {
      next.customName = "Enter a custom agent name.";
    }
    if (isCustom && !executable.trim()) {
      next.executable = "Enter an executable name or absolute path.";
    }
    const directoryError = validateRelativeDirectory(relativeDirectory.trim());
    if (directoryError) next.relativeDirectory = directoryError;
    setErrors(next);
    const first = Object.keys(next)[0] as FieldName | undefined;
    if (first) focusField(first);
    return first === undefined;
  }

  function focusField(field: FieldName) {
    const refs: Record<FieldName, RefObject<HTMLElement | null>> = {
      name: nameRef,
      agent: agentRef,
      customName: customNameRef,
      executable: executableRef,
      relativeDirectory: relativeDirectoryRef,
    };
    refs[field].current?.focus();
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (step === "details") {
      if (validate()) setStep("review");
      return;
    }
    if (inFlight.current) return;
    inFlight.current = true;
    setSubmitting(true);
    setRequestError(undefined);
    try {
      let effectiveAgentId = selectedAgentId;
      if (isCustom) {
        if (!createdCustomAgentId.current) {
          const customAgent = await onCreateCustomAgent({
            displayName: customName.trim(),
            command: {
              executable: executable.trim(),
              args: parsedCustomArgs,
              env: {},
            },
          });
          createdCustomAgentId.current = customAgent.id;
        }
        effectiveAgentId = createdCustomAgentId.current;
      }
      await onCreate({
        projectId: project.id,
        name: name.trim(),
        agentId: effectiveAgentId,
        isolation: worktreeMode === "current" ? "current" : "new_worktree",
        relativeDirectory: relativeDirectory.trim() || undefined,
      });
      onClose();
    } catch (error) {
      setRequestError(errorData(error));
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }

  const footer =
    step === "details" ? (
      <>
        <button className="button button--secondary" type="button" onClick={onClose}>
          Cancel
        </button>
        <button className="button button--primary" type="submit" form={formId}>
          Review Session
        </button>
      </>
    ) : (
      <>
        <button
          className="button button--secondary"
          type="button"
          disabled={submitting}
          onClick={() => {
            setStep("details");
            setRequestError(undefined);
          }}
        >
          Back
        </button>
        <button
          className="button button--primary"
          type="submit"
          form={formId}
          disabled={submitting}
          aria-busy={submitting}
        >
          {submitting ? "Creating…" : "Create Session"}
        </button>
      </>
    );

  return (
    <Dialog
      open={open}
      title="New Session"
      description={
        step === "details"
          ? "Configure the agent and daemon-owned working directory."
          : "Review every filesystem consequence before creation."
      }
      size="large"
      closeDisabled={submitting}
      initialFocusRef={nameRef}
      onClose={onClose}
      footer={footer}
    >
      <form
        id={formId}
        className="form-stack"
        noValidate
        onSubmit={(event) => void handleSubmit(event)}
      >
        <ol className="step-indicator" aria-label="Session creation progress">
          <li
            aria-current={step === "details" ? "step" : undefined}
            className={step === "details" ? "is-current" : "is-complete"}
          >
            <span>1</span> Details
          </li>
          <li
            aria-current={step === "review" ? "step" : undefined}
            className={step === "review" ? "is-current" : undefined}
          >
            <span>2</span> Review
          </li>
        </ol>
        {requestError ? <InlineRequestError error={requestError} /> : null}
        {step === "details" ? (
          <div className="session-form-grid">
            <fieldset className="form-section">
              <legend>Session</legend>
              <div className="field">
                <label htmlFor={nameId}>
                  Session name <span aria-hidden="true">*</span>
                </label>
                <input
                  ref={nameRef}
                  id={nameId}
                  className="text-input"
                  value={name}
                  maxLength={256}
                  required
                  aria-invalid={errors.name ? true : undefined}
                  aria-describedby={errors.name ? `${nameId}-error` : undefined}
                  placeholder="Implement authentication"
                  onChange={(event) => {
                    setName(event.target.value);
                    setErrors((current) => ({ ...current, name: undefined }));
                  }}
                />
                {errors.name ? (
                  <p id={`${nameId}-error`} className="field__error" role="alert">
                    {errors.name}
                  </p>
                ) : null}
              </div>
              <div className="field">
                <label htmlFor={agentId}>
                  Agent <span aria-hidden="true">*</span>
                </label>
                <select
                  ref={agentRef}
                  id={agentId}
                  className="select-input"
                  value={selectedAgentId}
                  aria-invalid={errors.agent ? true : undefined}
                  aria-describedby={errors.agent ? `${agentId}-error` : undefined}
                  onChange={(event) => {
                    setSelectedAgentId(event.target.value);
                    setErrors((current) => ({ ...current, agent: undefined }));
                  }}
                >
                  {agents.map((agent) => {
                    const detection = agentDetections.find(
                      (candidate) => candidate.agentId === agent.id,
                    );
                    const unavailable =
                      !agent.enabled || detection?.available === false;
                    return (
                      <option
                        value={agent.id}
                        key={agent.id}
                        disabled={unavailable}
                      >
                        {agent.displayName}
                        {unavailable ? " — unavailable" : ""}
                      </option>
                    );
                  })}
                  <option value={CUSTOM_AGENT_VALUE}>Custom agent…</option>
                </select>
                {errors.agent ? (
                  <p id={`${agentId}-error`} className="field__error" role="alert">
                    {errors.agent}
                  </p>
                ) : (
                  <p className="field__hint">
                    Agents run with their existing local authentication.
                  </p>
                )}
                {unavailableAgentNames.length ? (
                  <p className="field__error" role="status">
                    Executable not found: {unavailableAgentNames.join(", ")}. Choose an available agent or define a custom executable.
                  </p>
                ) : null}
              </div>
              {isCustom ? (
                <div className="nested-fields" aria-label="Custom agent definition">
                  <div className="field">
                    <label htmlFor={customNameId}>
                      Custom agent name <span aria-hidden="true">*</span>
                    </label>
                    <input
                      ref={customNameRef}
                      id={customNameId}
                      className="text-input"
                      value={customName}
                      maxLength={256}
                      aria-invalid={errors.customName ? true : undefined}
                      aria-describedby={
                        errors.customName ? `${customNameId}-error` : undefined
                      }
                      onChange={(event) => {
                        setCustomName(event.target.value);
                        createdCustomAgentId.current = undefined;
                      }}
                    />
                    {errors.customName ? (
                      <p id={`${customNameId}-error`} className="field__error" role="alert">
                        {errors.customName}
                      </p>
                    ) : null}
                  </div>
                  <div className="field">
                    <label htmlFor={executableId}>
                      Executable <span aria-hidden="true">*</span>
                    </label>
                    <input
                      ref={executableRef}
                      id={executableId}
                      className="text-input mono"
                      value={executable}
                      aria-invalid={errors.executable ? true : undefined}
                      aria-describedby={
                        errors.executable ? `${executableId}-error` : undefined
                      }
                      placeholder="agent-cli or /opt/bin/agent-cli"
                      onChange={(event) => {
                        setExecutable(event.target.value);
                        createdCustomAgentId.current = undefined;
                      }}
                    />
                    {errors.executable ? (
                      <p id={`${executableId}-error`} className="field__error" role="alert">
                        {errors.executable}
                      </p>
                    ) : null}
                  </div>
                  <div className="field">
                    <label htmlFor={customArgsId}>
                      Agent arguments <span className="field__optional">Optional</span>
                    </label>
                    <textarea
                      id={customArgsId}
                      className="text-area mono"
                      value={customArgs}
                      rows={3}
                      placeholder="One argument per line"
                      onChange={(event) => {
                        setCustomArgs(event.target.value);
                        createdCustomAgentId.current = undefined;
                      }}
                    />
                    <p className="field__hint">
                      Each non-empty line is one literal process argument. No shell parsing is used.
                    </p>
                  </div>
                </div>
              ) : null}
              <div className="field">
                <label htmlFor={relativeDirectoryId}>
                  Subdirectory <span className="field__optional">Optional</span>
                </label>
                <input
                  ref={relativeDirectoryRef}
                  id={relativeDirectoryId}
                  className="text-input mono"
                  value={relativeDirectory}
                  aria-invalid={errors.relativeDirectory ? true : undefined}
                  aria-describedby={`${relativeDirectoryId}-hint${
                    errors.relativeDirectory ? ` ${relativeDirectoryId}-error` : ""
                  }`}
                  placeholder="apps/desktop"
                  onChange={(event) => {
                    setRelativeDirectory(event.target.value);
                    setErrors((current) => ({
                      ...current,
                      relativeDirectory: undefined,
                    }));
                  }}
                />
                <p id={`${relativeDirectoryId}-hint`} className="field__hint">
                  Relative to <span className="mono">{projectRoot}</span>. Leave empty for the root.
                </p>
                {errors.relativeDirectory ? (
                  <p id={`${relativeDirectoryId}-error`} className="field__error" role="alert">
                    {errors.relativeDirectory}
                  </p>
                ) : null}
              </div>
            </fieldset>

            <fieldset className="form-section">
              <legend>Working tree</legend>
              <div className="segmented-options">
                <label className={worktreeMode === "current" ? "is-selected" : undefined}>
                  <input
                    type="radio"
                    name="worktree-mode"
                    value="current"
                    checked={worktreeMode === "current"}
                    onChange={() => setWorktreeMode("current")}
                  />
                  <span>
                    <strong>Current working tree</strong>
                    <small>Run in the registered repository.</small>
                  </span>
                </label>
                <label className={worktreeMode === "new" ? "is-selected" : undefined}>
                  <input
                    type="radio"
                    name="worktree-mode"
                    value="new"
                    checked={worktreeMode === "new"}
                    onChange={() => setWorktreeMode("new")}
                  />
                  <span>
                    <strong>New worktree</strong>
                    <small>Ask the daemon to create an isolated branch and directory.</small>
                  </span>
                </label>
              </div>
              {worktreeMode === "new" ? (
                <div className="nested-fields">
                  <div className="information-box information-box--warning">
                    <Icon name="warning" />
                    <p>
                      Creation adds a Git branch and a separate directory on disk. Deleting the session later removes neither one.
                    </p>
                  </div>
                  <dl className="review-list review-list--compact">
                    <div>
                      <dt>Base branch</dt>
                      <dd className="mono">{project.currentBranch ?? "Daemon-selected"}</dd>
                    </div>
                    <div>
                      <dt>Suggested branch</dt>
                      <dd className="mono">{suggestedBranch}</dd>
                    </div>
                  </dl>
                  <p className="field__hint">
                    These are previews. The daemon owns collision checks and returns the authoritative branch and path.
                  </p>
                </div>
              ) : null}
              <div className="information-box">
                <Icon name="terminal" />
                <p>
                  Per-session argument overrides are not accepted by the Beta v1 wire contract. Configure optional structured arguments on a custom agent.
                </p>
              </div>
            </fieldset>
          </div>
        ) : (
          <SessionReview
            project={project}
            name={name}
            agentName={
              isCustom
                ? customName
                : selectedAgent?.displayName ?? "Unknown agent"
            }
            executable={
              isCustom
                ? executable
                : selectedAgent?.command.executable
            }
            agentArgs={
              isCustom ? parsedCustomArgs : selectedAgent?.command.args ?? []
            }
            directory={effectiveDirectory}
            relativeDirectory={relativeDirectory.trim() || undefined}
            mode={worktreeMode}
            baseBranch={project.currentBranch}
            suggestedBranch={suggestedBranch}
          />
        )}
      </form>
    </Dialog>
  );
}

function SessionReview({
  project,
  name,
  agentName,
  executable,
  agentArgs,
  directory,
  relativeDirectory,
  mode,
  baseBranch,
  suggestedBranch,
}: {
  readonly project: Project;
  readonly name: string;
  readonly agentName: string;
  readonly executable?: string;
  readonly agentArgs: readonly string[];
  readonly directory: string;
  readonly relativeDirectory?: string;
  readonly mode: WorktreeMode;
  readonly baseBranch?: string;
  readonly suggestedBranch: string;
}) {
  return (
    <div className="review-panel">
      <dl className="review-list">
        <div><dt>Project</dt><dd>{project.name}</dd></div>
        <div><dt>Session</dt><dd>{name}</dd></div>
        <div>
          <dt>Agent</dt>
          <dd>{agentName}{executable ? <small className="mono">{executable}</small> : null}</dd>
        </div>
        <div><dt>Directory</dt><dd className="mono">{directory}</dd></div>
        {relativeDirectory ? <div><dt>Relative directory</dt><dd className="mono">{relativeDirectory}</dd></div> : null}
        <div><dt>Working tree</dt><dd>{mode === "current" ? "Use current working tree" : "Create a managed worktree"}</dd></div>
        {mode === "new" ? (
          <>
            <div><dt>Base branch preview</dt><dd className="mono">{baseBranch ?? "Daemon-selected"}</dd></div>
            <div><dt>Branch preview</dt><dd className="mono">{suggestedBranch}</dd></div>
          </>
        ) : null}
        <div><dt>Agent args</dt><dd>{agentArgs.length === 0 ? "None" : <code>{agentArgs.join(" · ")}</code>}</dd></div>
      </dl>
      {mode === "new" ? (
        <div className="review-consequence">
          <Icon name="worktree" />
          <div>
            <strong>A daemon-named branch and directory will be created.</strong>
            <p>Stop Process affects only the agent. Delete Session removes only metadata. Remove Worktree is a separate safety-checked action.</p>
          </div>
        </div>
      ) : (
        <div className="review-consequence">
          <Icon name="repository" />
          <div>
            <strong>The agent will run in the current working tree.</strong>
            <p>Existing uncommitted changes remain visible to the agent.</p>
          </div>
        </div>
      )}
    </div>
  );
}

function firstSelectableAgentId(
  agents: readonly AgentRecord[],
  detections: readonly AgentDetection[],
): string {
  return (
    agents.find((agent) => {
      const detection = detections.find(
        (candidate) => candidate.agentId === agent.id,
      );
      return agent.enabled && detection?.available !== false;
    })?.id ?? CUSTOM_AGENT_VALUE
  );
}

function validateRelativeDirectory(value: string): string | undefined {
  if (!value) return undefined;
  if (new TextEncoder().encode(value).length > 1_024) {
    return "Use at most 1024 UTF-8 bytes.";
  }
  if (value.startsWith("/") || value.startsWith("\\") || value.includes("\\")) {
    return "Use a relative path with forward slashes.";
  }
  if (/\p{Cc}/u.test(value)) {
    return "Control characters are not allowed.";
  }
  const components = value.split("/");
  if (components.some((component) => !component || component === "." || component === "..")) {
    return "Use only non-empty child directories; . and .. are not allowed.";
  }
  if (components[0]?.endsWith(":")) {
    return "Platform path prefixes are not allowed.";
  }
  return undefined;
}

function joinDisplayPath(root: string, relativeDirectory: string): string {
  if (!relativeDirectory) return root;
  return `${root.replace(/\/$/, "")}/${relativeDirectory}`;
}

function parseArgumentLines(value: string): readonly string[] {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}
