import { useEffect, useId, useRef } from "react";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  MouseEvent as ReactMouseEvent,
  ReactNode,
  RefObject,
  SyntheticEvent,
} from "react";

import { Icon } from "./Icon";

export type DialogSize = "small" | "medium" | "large";

/** Props for the controlled application dialog primitive. */
export interface DialogProps {
  /** Controls whether the dialog is mounted and displayed modally. */
  readonly open: boolean;
  /** Labels the dialog and appears in its header. */
  readonly title: string;
  /** Adds supporting context below the title. */
  readonly description?: string;
  /** Requests that the owner close the controlled dialog. */
  readonly onClose: () => void;
  /** Renders the dialog's primary content. */
  readonly children: ReactNode;
  /** Renders actions after the primary content. */
  readonly footer?: ReactNode;
  /** Selects the dialog's constrained content width. */
  readonly size?: DialogSize;
  /** Prevents Escape, backdrop, and close-button dismissal while true. */
  readonly closeDisabled?: boolean;
  /** Receives focus after the dialog opens when it points inside the dialog. */
  readonly initialFocusRef?: RefObject<HTMLElement | null>;
}

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

/** Renders a dependency-free modal with native dialog behavior and a safe fallback. */
export function Dialog({
  open,
  title,
  description,
  onClose,
  children,
  footer,
  size = "medium",
  closeDisabled = false,
  initialFocusRef,
}: DialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const usesNativeModalRef = useRef(false);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    if (!open) {
      return undefined;
    }

    const dialog = dialogRef.current;
    if (!dialog) {
      return undefined;
    }

    const activeElement = document.activeElement;
    restoreFocusRef.current =
      activeElement instanceof HTMLElement ? activeElement : null;
    usesNativeModalRef.current = openDialog(dialog);
    focusDialog(dialog, initialFocusRef?.current);

    return () => {
      closeDialog(dialog);
      restoreFocus(restoreFocusRef);
    };
  }, [initialFocusRef, open]);

  if (!open) {
    return null;
  }

  function requestClose() {
    if (!closeDisabled) {
      onClose();
    }
  }

  function handleCancel(event: SyntheticEvent<HTMLDialogElement>) {
    event.preventDefault();
    requestClose();
  }

  function handleBackdropClick(event: ReactMouseEvent<HTMLDialogElement>) {
    if (event.target === event.currentTarget) {
      requestClose();
    }
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLDialogElement>) {
    if (event.defaultPrevented || event.nativeEvent.isComposing) {
      return;
    }

    if (event.key === "Escape" && !usesNativeModalRef.current) {
      event.preventDefault();
      requestClose();
      return;
    }

    if (event.key === "Tab" && !usesNativeModalRef.current) {
      trapFallbackFocus(event);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className={`dialog dialog--${size}`}
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      aria-modal="true"
      tabIndex={-1}
      onCancel={handleCancel}
      onClick={handleBackdropClick}
      onKeyDown={handleKeyDown}
    >
      <header className="dialog__header">
        <div className="dialog__heading">
          <h2 id={titleId}>{title}</h2>
          {description ? <p id={descriptionId}>{description}</p> : null}
        </div>
        <button
          className="dialog__close"
          type="button"
          disabled={closeDisabled}
          aria-label={`Close ${title}`}
          onClick={requestClose}
        >
          <Icon name="close" />
        </button>
      </header>
      <div className="dialog__body">{children}</div>
      {footer ? <footer className="dialog__footer">{footer}</footer> : null}
    </dialog>
  );
}

/** Opens a modal natively when supported and otherwise applies fallback state. */
function openDialog(dialog: HTMLDialogElement): boolean {
  if (dialog.open) {
    return false;
  }

  if (typeof dialog.showModal === "function") {
    try {
      dialog.showModal();
      return true;
    } catch {
      dialog.setAttribute("open", "");
      return false;
    }
  }

  dialog.setAttribute("open", "");
  return false;
}

/** Closes either a native modal dialog or its attribute-based fallback. */
function closeDialog(dialog: HTMLDialogElement) {
  if (!dialog.open && !dialog.hasAttribute("open")) {
    return;
  }

  if (typeof dialog.close === "function") {
    try {
      dialog.close();
    } catch {
      dialog.removeAttribute("open");
    }
  } else {
    dialog.removeAttribute("open");
  }
}

/** Moves focus to the requested control or the first operable dialog control. */
function focusDialog(
  dialog: HTMLDialogElement,
  preferredElement: HTMLElement | null | undefined,
) {
  if (
    preferredElement &&
    dialog.contains(preferredElement) &&
    canReceiveFocus(preferredElement)
  ) {
    preferredElement.focus();
    return;
  }

  if (dialog.contains(document.activeElement)) {
    return;
  }

  const firstFocusableElement = getFocusableElements(dialog)[0];
  (firstFocusableElement ?? dialog).focus();
}

/** Keeps keyboard focus inside the non-native fallback dialog. */
function trapFallbackFocus(event: ReactKeyboardEvent<HTMLDialogElement>) {
  const dialog = event.currentTarget;
  const focusableElements = getFocusableElements(dialog);
  const firstElement = focusableElements[0];
  const lastElement = focusableElements[focusableElements.length - 1];

  if (!firstElement || !lastElement) {
    event.preventDefault();
    dialog.focus();
    return;
  }

  const activeElement = document.activeElement;
  const focusIsOutside = !dialog.contains(activeElement);

  if (event.shiftKey && (activeElement === firstElement || focusIsOutside)) {
    event.preventDefault();
    lastElement.focus();
    return;
  }

  if (!event.shiftKey && (activeElement === lastElement || focusIsOutside)) {
    event.preventDefault();
    firstElement.focus();
  }
}

/** Returns operable descendants in their document tab order. */
function getFocusableElements(dialog: HTMLDialogElement): readonly HTMLElement[] {
  return Array.from(
    dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter(canReceiveFocus);
}

/** Excludes controls that are disabled or hidden from assistive technology. */
function canReceiveFocus(element: HTMLElement): boolean {
  return (
    !element.matches(":disabled") &&
    element.getAttribute("aria-hidden") !== "true" &&
    !element.closest("[inert]")
  );
}

/** Restores focus only when the previous control remains connected. */
function restoreFocus(restoreFocusRef: RefObject<HTMLElement | null>) {
  const element = restoreFocusRef.current;
  restoreFocusRef.current = null;
  if (element?.isConnected) {
    element.focus();
  }
}
