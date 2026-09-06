import { useId, useLayoutEffect, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";

import { Icon } from "../../components/Icon";
import "../../../styles/prompt-composer.css";

/** A source whose current text can be explicitly copied into the draft. */
export interface PromptContextItem {
  readonly id: string;
  readonly title: string;
  readonly text: string;
}

/** Keys an empty composer can forward to its attached terminal. */
export type PromptTerminalKey =
  | "Enter"
  | "Tab"
  | "ArrowUp"
  | "ArrowDown"
  | "ArrowLeft"
  | "ArrowRight";

/** Controlled terminal input; the owner persists drafts and delivers input. */
export interface PromptComposerProps {
  readonly nodeId: string;
  readonly title: string;
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly onSend: (text: string) => Promise<void>;
  readonly onClose: () => void;
  /** Explains why input cannot be sent; drafting remains available. */
  readonly disabledReason?: string;
  readonly contextItems?: readonly PromptContextItem[];
  readonly onTerminalKey?: (key: PromptTerminalKey) => Promise<void>;
  /** False when the owner acknowledges sends with its own persistent revision check. */
  readonly clearOnSend?: boolean;
}

type PendingInput = { readonly kind: "prompt" } | {
  readonly kind: "key";
  readonly key: PromptTerminalKey;
};

/** Keeps each terminal's focus and in-flight input isolated when targets change. */
export function PromptComposer(props: PromptComposerProps) {
  return <TerminalPromptComposer key={props.nodeId} {...props} />;
}

function TerminalPromptComposer({
  nodeId,
  title,
  value,
  onChange,
  onSend,
  onClose,
  disabledReason,
  contextItems = [],
  onTerminalKey,
  clearOnSend = true,
}: PromptComposerProps) {
  const id = useId();
  const panelRef = useRef<HTMLElement>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const mountedRef = useRef(false);
  const composingRef = useRef(false);
  const busyRef = useRef(false);
  const draftRef = useRef({ value, onChange, revision: 0 });
  const [pending, setPending] = useState<PendingInput>();
  const [failed, setFailed] = useState<PendingInput>();
  const [status, setStatus] = useState("");

  useLayoutEffect(() => {
    const previous = draftRef.current;
    draftRef.current = {
      value,
      onChange,
      revision: previous.revision + (previous.value === value ? 0 : 1),
    };
  }, [value, onChange]);

  useLayoutEffect(() => {
    mountedRef.current = true;
    const panel = panelRef.current;
    const previousFocus = document.activeElement;
    editorRef.current?.focus();
    return () => {
      mountedRef.current = false;
      if (
        panel?.contains(document.activeElement)
        && previousFocus instanceof HTMLElement
        && previousFocus.isConnected
      ) {
        previousFocus.focus();
      }
    };
  }, []);

  function changeDraft(nextValue: string) {
    draftRef.current = {
      value: nextValue,
      onChange,
      revision: draftRef.current.revision + 1,
    };
    onChange(nextValue);
    setFailed(undefined);
    setStatus("");
  }

  async function sendPrompt() {
    if (disabledReason || busyRef.current || !value.trim()) return;
    const submitted = draftRef.current;
    busyRef.current = true;
    setPending({ kind: "prompt" });
    setFailed(undefined);
    setStatus("");
    try {
      await onSend(value);
      if (!mountedRef.current) return;
      const latest = draftRef.current;
      if (clearOnSend && latest.value === submitted.value && latest.revision === submitted.revision) {
        latest.onChange("");
      }
      setStatus(`Prompt sent to ${title}.`);
    } catch {
      if (mountedRef.current) setFailed({ kind: "prompt" });
    } finally {
      busyRef.current = false;
      if (mountedRef.current) setPending(undefined);
    }
  }

  async function sendTerminalKey(key: PromptTerminalKey) {
    if (disabledReason || busyRef.current || !onTerminalKey) return;
    busyRef.current = true;
    setPending({ kind: "key", key });
    setFailed(undefined);
    setStatus("");
    try {
      await onTerminalKey(key);
      if (mountedRef.current) setStatus(`Key sent to ${title}.`);
    } catch {
      if (mountedRef.current) setFailed({ kind: "key", key });
    } finally {
      busyRef.current = false;
      if (mountedRef.current) setPending(undefined);
    }
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void sendPrompt();
  }

  function handleEditorKey(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (
      composingRef.current || event.nativeEvent.isComposing
      || event.nativeEvent.keyCode === 229
      || event.ctrlKey || event.altKey || event.metaKey || event.shiftKey
    ) return;

    if (
      value === "" && onTerminalKey && !disabledReason && !busyRef.current
      && isTerminalKey(event.key)
    ) {
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) void sendTerminalKey(event.key);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) void sendPrompt();
    }
  }

  function insertContext(item: PromptContextItem) {
    const separator = value && !value.endsWith("\n\n") ? "\n\n" : "";
    changeDraft(`${value}${separator}Context snapshot: ${item.title}\n${item.text}\n`);
    setStatus(`Inserted a text snapshot from ${item.title}.`);
    editorRef.current?.focus();
  }

  const feedbackId = `${id}-feedback`;
  const disabledId = `${id}-disabled`;
  const helpId = `${id}-help`;

  return (
    <section
      ref={panelRef}
      className="prompt-composer"
      aria-labelledby={`${id}-title`}
      data-canvas-node-id={nodeId}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (
          event.key === "Escape" && !event.repeat && !composingRef.current
          && !event.nativeEvent.isComposing && event.nativeEvent.keyCode !== 229
        ) {
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <header className="prompt-composer__header">
        <div>
          <h3 id={`${id}-title`}><Icon name="pencil" /> Prompt Composer</h3>
          <p><Icon name="terminal" /> Send to <strong>{title}</strong></p>
        </div>
        <button
          type="button"
          className="prompt-composer__close"
          aria-label="Close Prompt Composer"
          onClick={onClose}
        ><Icon name="close" /></button>
      </header>
      <form onSubmit={handleSubmit}>
        <label className="prompt-composer__label" htmlFor={`${id}-draft`}>Prompt for {title}</label>
        <textarea
          ref={editorRef}
          id={`${id}-draft`}
          className="prompt-composer__editor"
          value={value}
          rows={5}
          placeholder="Write a prompt for this terminal…"
          aria-describedby={`${helpId} ${feedbackId}${disabledReason ? ` ${disabledId}` : ""}`}
          onChange={(event) => changeDraft(event.currentTarget.value)}
          onCompositionStart={() => { composingRef.current = true; }}
          onCompositionEnd={() => { composingRef.current = false; }}
          onKeyDown={handleEditorKey}
        />
        {contextItems.length > 0 ? (
          <details className="prompt-composer__context">
            <summary><Icon name="note" /> Insert context <span>({contextItems.length})</span></summary>
            <p>Copies the source text into this draft as a snapshot.</p>
            <ul>
              {contextItems.map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    onClick={() => insertContext(item)}
                    disabled={!item.text.trim()}
                    aria-label={`Insert context from ${item.title}`}
                  ><Icon name="plus" /><span>{item.title}</span></button>
                </li>
              ))}
            </ul>
          </details>
        ) : null}
        {disabledReason ? <p id={disabledId} className="prompt-composer__disabled">{disabledReason}</p> : null}
        <div id={feedbackId} className="prompt-composer__feedback">
          {failed ? (
            <p role="alert">
              Could not {failed.kind === "prompt" ? "send the prompt" : "deliver the key"}. Check the terminal and try again. Your draft is still here.
              {failed.kind === "key" ? (
                <button
                  type="button"
                  disabled={Boolean(disabledReason) || Boolean(pending)}
                  onClick={() => void sendTerminalKey(failed.key)}
                >Retry key</button>
              ) : null}
            </p>
          ) : null}
          <p role="status">{pending ? (pending.kind === "prompt" ? "Sending prompt…" : "Sending key…") : status}</p>
        </div>
        <footer className="prompt-composer__footer">
          <p id={helpId}>
            <kbd>Enter</kbd> send · <kbd>Shift+Enter</kbd> new line · <kbd>Esc</kbd> close
            {onTerminalKey ? <span>When empty, Enter, Tab and arrows go to the terminal. Shift+Tab leaves the editor.</span> : null}
          </p>
          <button
            type="submit"
            className="prompt-composer__send"
            disabled={Boolean(disabledReason) || Boolean(pending) || !value.trim()}
          ><Icon name="arrow-right" />{pending?.kind === "prompt" ? "Sending…" : failed?.kind === "prompt" ? "Try again" : "Send prompt"}</button>
        </footer>
      </form>
    </section>
  );
}

function isTerminalKey(key: string): key is PromptTerminalKey {
  return key === "Enter" || key === "Tab" || key === "ArrowUp"
    || key === "ArrowDown" || key === "ArrowLeft" || key === "ArrowRight";
}
