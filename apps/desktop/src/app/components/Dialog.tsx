import {
  useEffect,
  useId,
  useRef,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";

const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusableElements(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) =>
      element.tabIndex !== -1 &&
      !element.hasAttribute("disabled") &&
      element.getAttribute("aria-hidden") !== "true",
  );
}

interface DialogProps {
  readonly title: string;
  readonly open: boolean;
  readonly onClose: () => void;
  readonly children: ReactNode;
  readonly describedBy?: string;
  readonly initialFocusRef?: RefObject<HTMLElement | null>;
}

/**
 * Modal dialog with a focus trap, Escape to dismiss, and focus restoration.
 */
export function Dialog({
  title,
  open,
  onClose,
  children,
  describedBy,
  initialFocusRef,
}: DialogProps) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const active = document.activeElement;
    previouslyFocused.current =
      active instanceof HTMLElement ? active : null;
    const dialog = dialogRef.current;
    if (!dialog) {
      return undefined;
    }
    const dialogElement = dialog;

    function focusInitialControl() {
      const elements = focusableElements(dialogElement);
      const preferred = initialFocusRef?.current;
      const initial =
        preferred && elements.includes(preferred)
          ? preferred
          : (elements[0] ?? dialogElement);
      initial.focus();
    }

    function containProgrammaticFocus(event: FocusEvent) {
      if (
        event.target instanceof Node &&
        !dialogElement.contains(event.target)
      ) {
        focusInitialControl();
      }
    }

    focusInitialControl();
    document.addEventListener("focusin", containProgrammaticFocus);

    return () => {
      document.removeEventListener("focusin", containProgrammaticFocus);
      if (previouslyFocused.current?.isConnected) {
        previouslyFocused.current.focus();
      }
    };
  }, [initialFocusRef, open]);

  if (!open || typeof document === "undefined") {
    return null;
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key !== "Tab") {
      return;
    }
    const dialog = dialogRef.current;
    if (!dialog) {
      return;
    }
    const elements = focusableElements(dialog);
    if (elements.length === 0) {
      event.preventDefault();
      dialog.focus();
      return;
    }
    const first = elements[0];
    const last = elements[elements.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
      return;
    }
    if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return createPortal(
    <div className="dialog-backdrop">
      <div
        ref={dialogRef}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={describedBy}
        tabIndex={-1}
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId} className="dialog__title">
          {title}
        </h2>
        {children}
      </div>
    </div>,
    document.body,
  );
}
