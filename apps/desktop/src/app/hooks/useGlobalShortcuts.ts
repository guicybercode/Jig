import { useEffect } from "react";

import type { AppPlatform } from "../../ipc/client";

export type SessionShortcutNumber = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;

/** Handlers and platform context for application-level shortcuts. */
export interface GlobalShortcutOptions {
  /** Disables all registered shortcuts without changing handlers. */
  readonly enabled?: boolean;
  /** Uses Command on macOS and Control on Linux. */
  readonly platform?: AppPlatform;
  /** Handles Command/Control+K. */
  readonly onOpenCommandPalette?: () => void;
  /** Handles Command/Control+T. */
  readonly onNewSession?: () => void;
  /** Handles Command/Control+Shift+G. */
  readonly onOpenGrid?: () => void;
  /** Handles Command/Control+1 through 9 using a one-based session number. */
  readonly onFocusSession?: (sessionNumber: SessionShortcutNumber) => void;
}

const TERMINAL_SELECTOR = "[data-terminal-root]";

/** Registers keyboard-first application shortcuts without capturing terminal input. */
export function useGlobalShortcuts({
  enabled = true,
  platform,
  onOpenCommandPalette,
  onNewSession,
  onOpenGrid,
  onFocusSession,
}: GlobalShortcutOptions): void {
  const shortcutPlatform = platform ?? detectShortcutPlatform();

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (
        event.defaultPrevented ||
        event.isComposing ||
        event.repeat ||
        isInsideTerminal(event) ||
        !hasPrimaryModifier(event, shortcutPlatform)
      ) {
        return;
      }

      const key = event.key.toLowerCase();

      if (!event.shiftKey && key === "k" && onOpenCommandPalette) {
        event.preventDefault();
        onOpenCommandPalette();
        return;
      }

      if (!event.shiftKey && key === "t" && onNewSession) {
        event.preventDefault();
        onNewSession();
        return;
      }

      if (event.shiftKey && key === "g" && onOpenGrid) {
        event.preventDefault();
        onOpenGrid();
        return;
      }

      if (!event.shiftKey && onFocusSession) {
        const sessionNumber = getSessionShortcutNumber(event.key);
        if (sessionNumber) {
          event.preventDefault();
          onFocusSession(sessionNumber);
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    enabled,
    onFocusSession,
    onNewSession,
    onOpenCommandPalette,
    onOpenGrid,
    shortcutPlatform,
  ]);
}

/** Determines the desktop platform when the IPC layer has not supplied it yet. */
function detectShortcutPlatform(): AppPlatform {
  if (typeof navigator === "undefined") {
    return "unknown";
  }

  const platformText = `${navigator.platform} ${navigator.userAgent}`.toLowerCase();
  if (platformText.includes("mac")) {
    return "macos";
  }
  if (platformText.includes("linux")) {
    return "linux";
  }
  return "unknown";
}

/** Accepts only the platform's primary modifier and rejects Alt chords. */
function hasPrimaryModifier(
  event: KeyboardEvent,
  platform: AppPlatform,
): boolean {
  if (event.altKey) {
    return false;
  }
  if (platform === "macos") {
    return event.metaKey && !event.ctrlKey;
  }
  if (platform === "linux") {
    return event.ctrlKey && !event.metaKey;
  }
  return event.metaKey !== event.ctrlKey;
}

/** Detects terminal ownership even when the event crossed a shadow boundary. */
function isInsideTerminal(event: KeyboardEvent): boolean {
  return event.composedPath().some(
    (eventTarget) =>
      eventTarget instanceof Element &&
      (eventTarget.matches(TERMINAL_SELECTOR) ||
        Boolean(eventTarget.closest(TERMINAL_SELECTOR))),
  );
}

/** Parses a supported session shortcut without an unsafe numeric assertion. */
function getSessionShortcutNumber(key: string): SessionShortcutNumber | null {
  switch (key) {
    case "1":
      return 1;
    case "2":
      return 2;
    case "3":
      return 3;
    case "4":
      return 4;
    case "5":
      return 5;
    case "6":
      return 6;
    case "7":
      return 7;
    case "8":
      return 8;
    case "9":
      return 9;
    default:
      return null;
  }
}
