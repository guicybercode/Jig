import type { ReactNode } from "react";

interface IconProps {
  readonly name:
    | "branch"
    | "copy"
    | "folder"
    | "plus"
    | "repository"
    | "session"
    | "terminal";
}

/** Renders the shared, outline-style application icon set. */
export function Icon({ name }: IconProps) {
  return (
    <svg
      className="icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {getIconPaths(name)}
    </svg>
  );
}

/** Selects the vector paths for a semantic icon name. */
function getIconPaths(name: IconProps["name"]): ReactNode {
  switch (name) {
    case "branch":
      return (
        <>
          <circle cx="6" cy="5" r="2" />
          <circle cx="18" cy="6" r="2" />
          <circle cx="6" cy="19" r="2" />
          <path d="M6 7v10M8 8c2 0 3.5 0 5-1l3-1" />
        </>
      );
    case "copy":
      return (
        <>
          <rect x="8" y="8" width="12" height="12" rx="2" />
          <path d="M4 16V6a2 2 0 0 1 2-2h10" />
        </>
      );
    case "folder":
      return (
        <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v8A2.5 2.5 0 0 1 18.5 19h-14A1.5 1.5 0 0 1 3 17.5v-11Z" />
      );
    case "plus":
      return <path d="M12 5v14M5 12h14" />;
    case "repository":
      return (
        <>
          <path d="M5 4.5A2.5 2.5 0 0 1 7.5 2H19v18H7.5A2.5 2.5 0 0 0 5 22V4.5Z" />
          <path d="M5 18.5A2.5 2.5 0 0 1 7.5 16H19M9 6h6" />
        </>
      );
    case "session":
    case "terminal":
      return (
        <>
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="m7 9 3 3-3 3M13 15h4" />
        </>
      );
  }
}
