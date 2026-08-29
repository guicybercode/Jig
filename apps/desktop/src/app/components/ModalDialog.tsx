import {
  useLayoutEffect,
  useRef,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

interface ModalDialogProps {
  readonly labelledBy: string;
  readonly describedBy?: string;
  readonly initialFocusRef?: RefObject<HTMLElement | null>;
  readonly onDismiss: () => void;
  readonly children: ReactNode;
}

/** Provides focus containment, Escape dismissal, and focus restoration for modal content. */
export function ModalDialog({
  labelledBy,
  describedBy,
  initialFocusRef,
  onDismiss,
  children,
}: ModalDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) {
      return undefined;
    }
    const dialogElement = dialog;

    const previouslyFocused =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    function focusInitialControl() {
      const focusableControls = getFocusableControls(dialogElement);
      const preferredControl = initialFocusRef?.current;
      let initialControl = focusableControls[0] ?? dialogElement;
      if (preferredControl && focusableControls.includes(preferredControl)) {
        initialControl = preferredControl;
      }
      initialControl.focus();
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
      if (previouslyFocused?.isConnected) {
        previouslyFocused.focus();
      }
    };
  }, [initialFocusRef]);

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onDismiss();
      return;
    }

    if (event.key !== "Tab") {
      return;
    }

    const dialog = dialogRef.current;
    if (!dialog) {
      return;
    }

    const controls = getFocusableControls(dialog);
    if (controls.length === 0) {
      event.preventDefault();
      dialog.focus();
      return;
    }

    const firstControl = controls[0];
    const lastControl = controls[controls.length - 1];
    if (!firstControl || !lastControl) {
      return;
    }

    const activeElement = document.activeElement;
    const focusIsOutside = !dialog.contains(activeElement);

    if (event.shiftKey && (activeElement === firstControl || focusIsOutside)) {
      event.preventDefault();
      lastControl.focus();
      return;
    }

    if (!event.shiftKey && (activeElement === lastControl || focusIsOutside)) {
      event.preventDefault();
      firstControl.focus();
    }
  }

  return (
    <div className="dialog-backdrop">
      <div
        ref={dialogRef}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        tabIndex={-1}
        onKeyDown={handleKeyDown}
      >
        {children}
      </div>
    </div>
  );
}

/** Returns controls participating in the dialog's sequential keyboard order. */
function getFocusableControls(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
}
