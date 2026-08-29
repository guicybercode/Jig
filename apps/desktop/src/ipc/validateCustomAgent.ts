import type { CustomAgentInput } from "./agentTypes";

export interface FieldErrors {
  displayName?: string;
  executable?: string;
  args?: string;
  env?: string;
  defaultCwd?: string;
}

/** Validates a custom agent form without invoking a shell. */
export function validateCustomAgent(input: CustomAgentInput): FieldErrors {
  const errors: FieldErrors = {};
  if (input.displayName.trim().length === 0) {
    errors.displayName = "Name is required.";
  }
  const executable = input.executable.trim();
  if (executable.length === 0) {
    errors.executable = "Executable is required.";
  } else if (executable.includes("\0")) {
    errors.executable = "Executable must not contain a NUL byte.";
  } else if (
    executable.startsWith("~") &&
    executable !== "~" &&
    !executable.startsWith("~/")
  ) {
    errors.executable = "Use ~/ or an absolute path, not a ~user path.";
  } else if (
    !executable.startsWith("/") &&
    !executable.startsWith("~/") &&
    executable !== "~" &&
    !executable.includes("${") &&
    executable.includes("/")
  ) {
    errors.executable =
      "Use an absolute path, a ~/ path, a placeholder, or a bare command name.";
  }

  if (input.args.some((argument) => argument.includes("\0"))) {
    errors.args = "Arguments must not contain a NUL byte.";
  }

  const keys = input.env.map((entry) => entry.key.trim());
  if (input.env.some((entry) => entry.key.trim().length === 0 && entry.value.length > 0)) {
    errors.env = "Environment variable names are required.";
  }
  if (keys.some((key) => key.includes("="))) {
    errors.env = "Environment variable names must not contain '='.";
  }
  if (input.env.some((entry) => entry.key.includes("\0") || entry.value.includes("\0"))) {
    errors.env = "Environment entries must not contain a NUL byte.";
  }

  const cwd = input.defaultCwd.trim();
  if (
    cwd.length > 0 &&
    !cwd.startsWith("/") &&
    !cwd.startsWith("~/") &&
    cwd !== "~" &&
    !cwd.includes("${")
  ) {
    errors.defaultCwd = "Directory must be absolute, ~/…, or a placeholder.";
  }

  return errors;
}

export function hasFieldErrors(errors: FieldErrors): boolean {
  return Object.keys(errors).length > 0;
}
