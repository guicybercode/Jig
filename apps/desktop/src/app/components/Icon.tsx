import type { ReactNode } from "react";

export type IconName =
  | "arrow-left"
  | "arrow-right"
  | "branch"
  | "browser"
  | "check"
  | "chevron-down"
  | "chevron-right"
  | "chevronDown"
  | "chevronRight"
  | "close"
  | "copy"
  | "diagnostics"
  | "external-link"
  | "folder"
  | "grid"
  | "layers"
  | "link"
  | "map"
  | "menu"
  | "monitor"
  | "more"
  | "note"
  | "pencil"
  | "play"
  | "pointer"
  | "plus"
  | "refresh"
  | "repository"
  | "sidebar"
  | "search"
  | "session"
  | "settings"
  | "stop"
  | "terminal"
  | "trash"
  | "warning"
  | "worktree"
  | "zoom-in"
  | "zoom-out";

/** Props for the shared outline icon set. */
export interface IconProps {
  /** Selects a semantic glyph from the application icon set. */
  readonly name: IconName;
  /** Adds a class while preserving the shared icon class. */
  readonly className?: string;
  /** Gives a standalone meaningful icon an accessible name. */
  readonly title?: string;
}

/** Renders the shared, outline-style application icon set. */
export function Icon({ name, className, title }: IconProps) {
  return (
    <svg
      className={className ? `icon ${className}` : "icon"}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      focusable="false"
    >
      {title ? <title>{title}</title> : null}
      {getIconPaths(name)}
    </svg>
  );
}

/** Selects the vector paths for a semantic icon name. */
function getIconPaths(name: IconName): ReactNode {
  switch (name) {
    case "arrow-left":
      return <path d="m15 18-6-6 6-6M9 12h10" />;
    case "arrow-right":
      return <path d="m9 6 6 6-6 6M5 12h10" />;
    case "branch":
      return (
        <>
          <circle cx="6" cy="5" r="2" />
          <circle cx="18" cy="6" r="2" />
          <circle cx="6" cy="19" r="2" />
          <path d="M6 7v10M8 8c2 0 3.5 0 5-1l3-1" />
        </>
      );
    case "browser":
      return (
        <>
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3c2.4 2.5 3.5 5.5 3.5 9S14.4 18.5 12 21c-2.4-2.5-3.5-5.5-3.5-9S9.6 5.5 12 3Z" />
        </>
      );
    case "check":
      return <path d="m5 12 4 4L19 6" />;
    case "chevron-down":
    case "chevronDown":
      return <path d="m6 9 6 6 6-6" />;
    case "chevron-right":
    case "chevronRight":
      return <path d="m9 6 6 6-6 6" />;
    case "close":
      return <path d="M6 6l12 12M18 6 6 18" />;
    case "copy":
      return (
        <>
          <rect x="8" y="8" width="11" height="11" rx="2" />
          <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
        </>
      );
    case "diagnostics":
      return (
        <>
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="M6 12h3l2-4 3 8 2-4h2" />
        </>
      );
    case "external-link":
      return (
        <>
          <path d="M14 4h6v6M20 4l-9 9" />
          <path d="M18 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5" />
        </>
      );
    case "folder":
      return (
        <path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v8A2.5 2.5 0 0 1 18.5 19h-14A1.5 1.5 0 0 1 3 17.5v-11Z" />
      );
    case "grid":
      return (
        <>
          <rect x="3" y="3" width="7" height="7" rx="1" />
          <rect x="14" y="3" width="7" height="7" rx="1" />
          <rect x="3" y="14" width="7" height="7" rx="1" />
          <rect x="14" y="14" width="7" height="7" rx="1" />
        </>
      );
    case "layers":
      return (
        <>
          <path d="m12 3 8.5 4.75L12 12.5 3.5 7.75 12 3Z" />
          <path d="m5.5 11.25-2 1.1L12 17l8.5-4.65-2-1.1M5.5 15.75l-2 1.1L12 21.5l8.5-4.65-2-1.1" />
        </>
      );
    case "link":
      return (
        <>
          <path d="M10.2 13.8a4 4 0 0 0 5.7 0l2.1-2.1A4 4 0 1 0 12.3 6l-1.2 1.2" />
          <path d="M13.8 10.2a4 4 0 0 0-5.7 0L6 12.3A4 4 0 1 0 11.7 18l1.2-1.2" />
        </>
      );
    case "map":
      return (
        <>
          <path d="m3 5 5-2 8 3 5-2v15l-5 2-8-3-5 2V5Z" />
          <path d="M8 3v15M16 6v15" />
        </>
      );
    case "menu":
      return <path d="M4 6h16M4 12h16M4 18h16" />;
    case "more":
      return (
        <>
          <circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" />
          <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
          <circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" />
        </>
      );
    case "monitor":
      return (
        <>
          <rect x="3" y="4" width="18" height="13" rx="2" />
          <path d="M8 21h8M12 17v4" />
        </>
      );
    case "note":
      return (
        <>
          <path d="M5 3h11l3 3v15H5V3Z" />
          <path d="M16 3v4h4M8 11h8M8 15h8M8 7h4" />
        </>
      );
    case "pencil":
      return (
        <>
          <path d="m4 20 4.5-1 10-10a2.1 2.1 0 0 0-3-3l-10 10L4 20Z" />
          <path d="m14 7 3 3M5.5 16l3 3" />
        </>
      );
    case "play":
      return <path d="m8 5 11 7-11 7V5Z" />;
    case "pointer":
      return (
        <path d="M5 3.5 18.5 13l-6 .8 3.8 6.2-2.8 1.5-3.7-6.1L6 20 5 3.5Z" />
      );
    case "plus":
      return <path d="M12 5v14M5 12h14" />;
    case "refresh":
      return (
        <>
          <path d="M20 7v5h-5" />
          <path d="M4 17v-5h5" />
          <path d="M6.1 8a7 7 0 0 1 11.4-2L20 8M4 16l2.5 2a7 7 0 0 0 11.4-2" />
        </>
      );
    case "repository":
      return (
        <>
          <path d="M5 4.5A2.5 2.5 0 0 1 7.5 2H19v18H7.5A2.5 2.5 0 0 0 5 22V4.5Z" />
          <path d="M5 18.5A2.5 2.5 0 0 1 7.5 16H19M9 6h6" />
        </>
      );
    case "sidebar":
      return (
        <>
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="M9 4v16M5.5 8h1M5.5 12h1" />
        </>
      );
    case "search":
      return (
        <>
          <circle cx="11" cy="11" r="7" />
          <path d="m16 16 4 4" />
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
    case "settings":
      return (
        <>
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-1.6v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z" />
        </>
      );
    case "stop":
      return <rect x="6" y="6" width="12" height="12" rx="1" />;
    case "trash":
      return (
        <>
          <path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 11v6M14 11v6" />
        </>
      );
    case "warning":
      return (
        <>
          <path d="M10.3 4.1 2.8 17a2 2 0 0 0 1.7 3h15a2 2 0 0 0 1.7-3L13.7 4.1a2 2 0 0 0-3.4 0Z" />
          <path d="M12 9v4M12 17h.01" />
        </>
      );
    case "worktree":
      return (
        <>
          <circle cx="6" cy="5" r="2" />
          <circle cx="6" cy="19" r="2" />
          <circle cx="18" cy="12" r="2" />
          <path d="M6 7v10M8 12h8M12 12V7h4" />
        </>
      );
    case "zoom-in":
      return (
        <>
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m15.5 15.5 4.5 4.5M10.5 7.5v6M7.5 10.5h6" />
        </>
      );
    case "zoom-out":
      return (
        <>
          <circle cx="10.5" cy="10.5" r="6.5" />
          <path d="m15.5 15.5 4.5 4.5M7.5 10.5h6" />
        </>
      );
  }
}
