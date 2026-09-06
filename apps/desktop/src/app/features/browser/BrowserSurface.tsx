import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { FormEvent } from "react";

import { Icon } from "../../components/Icon";
import {
  normalizeBrowserNavigationUrl,
  normalizeBrowserUrl,
} from "../canvas/canvas-state";
import {
  BrowserRuntimeError,
  defaultBrowserRuntime,
  type BrowserBounds,
  type BrowserRuntime,
  type BrowserRuntimeEvent,
  type BrowserRuntimeUnsubscribe,
} from "./browser-runtime";

const INVALID_ADDRESS_MESSAGE =
  "Enter a valid HTTP or HTTPS address without embedded credentials.";
const RUNTIME_ACTION_ERROR = "The integrated browser could not complete this action.";

/** Props for one browser card's DOM chrome and native surface reservation. */
export interface BrowserSurfaceProps {
  /** Stable canvas node identifier; native webview labels remain runtime-private. */
  readonly nodeId: string;
  /** Last valid, persistable address owned by the canvas document. */
  readonly url: string;
  /** Accessible name for the complete browser region. */
  readonly accessibleLabel?: string;
  /** Whether this selected card owns the single native browser surface. */
  readonly active?: boolean;
  /** Whether the native surface may be shown above its DOM reservation. */
  readonly visible?: boolean;
  /** Explains why an active surface is hidden, such as non-100% canvas zoom. */
  readonly unavailableReason?: string;
  /** Injectable project runtime; defaults to the shared Tauri-backed runtime. */
  readonly runtime?: BrowserRuntime;
  /** Requests selection of this card; the canvas controls `active`. */
  readonly onActivate: () => void;
  /** Receives only a redacted, persistable address explicitly submitted by the user. */
  readonly onNavigate: (url: string) => void | Promise<void>;
  /** Delegates opening a valid address outside the application. */
  readonly onOpenExternal: (url: string) => void | Promise<void>;
  /** Optional integration class; browser styling remains feature-local. */
  readonly className?: string;
}

/**
 * Renders accessible browser controls around a reserved native-webview slot.
 *
 * Remote content is never rendered in an iframe. In a regular browser or test
 * process the slot remains a safe explanatory placeholder.
 */
export function BrowserSurface({
  nodeId,
  url,
  accessibleLabel = "Integrated browser",
  active = false,
  visible = true,
  unavailableReason,
  runtime = defaultBrowserRuntime,
  onActivate,
  onNavigate,
  onOpenExternal,
  className,
}: BrowserSurfaceProps) {
  const inputId = useId();
  const surfaceDescriptionId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const activeRef = useRef(active);
  const visibleRef = useRef(visible);
  const openedRef = useRef(false);
  const openingRef = useRef<Promise<void> | null>(null);
  const latestBoundsRef = useRef<BrowserBounds | null>(null);
  const geometryAllowsVisibilityRef = useRef(false);
  const currentUrlRef = useRef(normalizeBrowserUrl(url));
  const requestedUrlRef = useRef<string | null>(null);
  const openedUrlRef = useRef<string | null>(null);
  const pendingPersistedUrlRef = useRef<string | null>(null);
  const lastPropUrlRef = useRef(currentUrlRef.current);
  const lifecycleGenerationRef = useRef(0);
  const scheduleGeometryRef = useRef<() => void>(() => undefined);
  const callbacksRef = useRef({ onNavigate, onOpenExternal });
  const [address, setAddress] = useState(url);
  const [currentUrl, setCurrentUrl] = useState(currentUrlRef.current);
  const [geometryAllowsVisibility, setGeometryAllowsVisibility] =
    useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runtimeAvailable = runtime.isAvailable();
  const canControlNative =
    active &&
    visible &&
    geometryAllowsVisibility &&
    runtimeAvailable &&
    currentUrl.length > 0;

  useLayoutEffect(() => {
    activeRef.current = active;
    visibleRef.current = visible;
    callbacksRef.current = { onNavigate, onOpenExternal };
  }, [active, onNavigate, onOpenExternal, visible]);

  const reportRuntimeFailure = useCallback((cause: unknown) => {
    if (cause instanceof BrowserRuntimeError) {
      setError(cause.message);
    } else {
      setError(RUNTIME_ACTION_ERROR);
    }
    setLoading(false);
  }, []);

  const hideNativeSurface = useCallback((): void => {
    const bounds = latestBoundsRef.current;
    const surfaceMayExist = openedRef.current || openingRef.current !== null;
    if (!bounds || !surfaceMayExist) return;

    void runtime
      .update({ nodeId, bounds, visible: false })
      .catch(() => undefined);
  }, [nodeId, runtime]);

  useLayoutEffect(() => {
    if (active && !visible) hideNativeSurface();
  }, [active, hideNativeSurface, visible]);

  const ensureNativeOpen = useCallback(
    async (nextUrl: string): Promise<void> => {
      if (!activeRef.current || !runtime.isAvailable()) return;

      if (openedRef.current) {
        if (openedUrlRef.current === nextUrl) return;
        requestedUrlRef.current = nextUrl;
        setLoading(true);
        try {
          await runtime.navigate({ nodeId, url: nextUrl });
          openedUrlRef.current = nextUrl;
        } catch (cause) {
          reportRuntimeFailure(cause);
        }
        return;
      }

      const pendingOpen = openingRef.current;
      if (pendingOpen) {
        try {
          await pendingOpen;
        } catch {
          return;
        }
        if (!activeRef.current) return;
        if (openingRef.current === pendingOpen) openingRef.current = null;
        if (!openedRef.current || openedUrlRef.current !== nextUrl) {
          await ensureNativeOpen(nextUrl);
        }
        return;
      }

      const geometry = readBrowserGeometry(surfaceRef.current);
      const bounds = latestBoundsRef.current ?? geometry?.bounds;
      if (!bounds) return;
      latestBoundsRef.current = bounds;
      if (geometry) {
        geometryAllowsVisibilityRef.current = geometry.allowsVisibility;
        setGeometryAllowsVisibility(geometry.allowsVisibility);
      }
      requestedUrlRef.current = nextUrl;
      const generation = lifecycleGenerationRef.current;
      setLoading(true);

      const opening = runtime.open({
        nodeId,
        url: nextUrl,
        bounds,
        visible:
          visibleRef.current && geometryAllowsVisibilityRef.current,
      });
      openingRef.current = opening;
      try {
        await opening;
        if (
          generation !== lifecycleGenerationRef.current ||
          !activeRef.current
        ) {
          return;
        }
        openedRef.current = true;
        openedUrlRef.current = requestedUrlRef.current ?? nextUrl;
        scheduleGeometryRef.current();
      } catch (cause) {
        if (generation === lifecycleGenerationRef.current) {
          reportRuntimeFailure(cause);
        }
      } finally {
        if (openingRef.current === opening) openingRef.current = null;
      }
    },
    [nodeId, reportRuntimeFailure, runtime],
  );

  const handleRuntimeEvent = useCallback(
    (event: BrowserRuntimeEvent): void => {
      if (event.nodeId !== nodeId) return;
      if (event.type === "load-state") {
        setLoading(event.status === "started");
        if (event.status === "started") {
          setError((current) =>
            current === INVALID_ADDRESS_MESSAGE ? current : null,
          );
        }
        return;
      }

      const normalizedUrl = normalizeBrowserNavigationUrl(event.url);
      if (!normalizedUrl) {
        setError(RUNTIME_ACTION_ERROR);
        return;
      }
      currentUrlRef.current = normalizedUrl;
      requestedUrlRef.current = normalizedUrl;
      openedUrlRef.current = normalizedUrl;
      setCurrentUrl(normalizedUrl);
      if (inputRef.current !== inputRef.current?.ownerDocument.activeElement) {
        setAddress(normalizedUrl);
      }
    },
    [nodeId],
  );

  useEffect(() => {
    if (!active || !runtimeAvailable) return undefined;

    lifecycleGenerationRef.current += 1;
    let cancelled = false;
    let unsubscribe: BrowserRuntimeUnsubscribe | null = null;

    void (async () => {
      try {
        if (runtime.subscribe) {
          const stopListening = await runtime.subscribe(
            nodeId,
            handleRuntimeEvent,
          );
          if (cancelled) {
            stopListening();
            return;
          }
          unsubscribe = stopListening;
        }
        const nextUrl = currentUrlRef.current;
        if (!cancelled && nextUrl) await ensureNativeOpen(nextUrl);
      } catch (cause) {
        if (!cancelled) reportRuntimeFailure(cause);
      }
    })();

    return () => {
      cancelled = true;
      lifecycleGenerationRef.current += 1;
      unsubscribe?.();
      openedRef.current = false;
      openedUrlRef.current = null;
      requestedUrlRef.current = null;
      setLoading(false);
      void runtime.close({ nodeId }).catch(() => undefined);
    };
  }, [
    active,
    ensureNativeOpen,
    handleRuntimeEvent,
    nodeId,
    reportRuntimeFailure,
    runtime,
    runtimeAvailable,
  ]);

  useEffect(() => {
    const normalizedUrl = normalizeBrowserUrl(url);
    if (!normalizedUrl) {
      if (url.trim()) setError(INVALID_ADDRESS_MESSAGE);
      return;
    }

    const propUrlChanged = lastPropUrlRef.current !== normalizedUrl;
    lastPropUrlRef.current = normalizedUrl;
    if (pendingPersistedUrlRef.current === normalizedUrl) {
      pendingPersistedUrlRef.current = null;
      return;
    }
    pendingPersistedUrlRef.current = null;
    currentUrlRef.current = normalizedUrl;
    setCurrentUrl(normalizedUrl);
    if (inputRef.current !== inputRef.current?.ownerDocument.activeElement) {
      setAddress(normalizedUrl);
    }
    if (
      active &&
      runtimeAvailable &&
      propUrlChanged &&
      requestedUrlRef.current !== normalizedUrl
    ) {
      void ensureNativeOpen(normalizedUrl);
    }
  }, [active, ensureNativeOpen, runtimeAvailable, url]);

  useLayoutEffect(() => {
    const element = surfaceRef.current;
    if (!element || !active) {
      geometryAllowsVisibilityRef.current = false;
      setGeometryAllowsVisibility(false);
      return undefined;
    }
    const view = element.ownerDocument.defaultView;
    let disposed = false;
    let frameId: number | null = null;

    const schedule = (): void => {
      if (disposed || frameId !== null) return;
      frameId = requestFrame(view, () => {
        frameId = null;
        const geometry = readBrowserGeometry(element);
        if (!geometry) {
          geometryAllowsVisibilityRef.current = false;
          setGeometryAllowsVisibility(false);
          const lastBounds = latestBoundsRef.current;
          if (!activeRef.current || !openedRef.current || !lastBounds) return;
          void runtime
            .update({ nodeId, bounds: lastBounds, visible: false })
            .catch(reportRuntimeFailure);
          return;
        }
        latestBoundsRef.current = geometry.bounds;
        geometryAllowsVisibilityRef.current = geometry.allowsVisibility;
        setGeometryAllowsVisibility(geometry.allowsVisibility);
        if (!activeRef.current || !openedRef.current) return;
        void runtime
          .update({
            nodeId,
            bounds: geometry.bounds,
            visible:
              visibleRef.current && geometry.allowsVisibility,
          })
          .catch(reportRuntimeFailure);
      });
    };
    scheduleGeometryRef.current = schedule;

    const ResizeObserverConstructor = view?.ResizeObserver;
    const resizeObserver = ResizeObserverConstructor
      ? new ResizeObserverConstructor(schedule)
      : null;
    resizeObserver?.observe(element);
    const MutationObserverConstructor = view?.MutationObserver;
    const mutationObserver = MutationObserverConstructor
      ? new MutationObserverConstructor(schedule)
      : null;
    if (element.ownerDocument.body) {
      mutationObserver?.observe(element.ownerDocument.body, {
        attributes: true,
        attributeFilter: ["aria-hidden", "class", "open", "style"],
        childList: true,
        subtree: true,
      });
    }
    view?.addEventListener("scroll", schedule, true);
    view?.addEventListener("resize", schedule);
    schedule();

    return () => {
      hideNativeSurface();
      disposed = true;
      scheduleGeometryRef.current = () => undefined;
      resizeObserver?.disconnect();
      mutationObserver?.disconnect();
      view?.removeEventListener("scroll", schedule, true);
      view?.removeEventListener("resize", schedule);
      if (frameId !== null) cancelFrame(view, frameId);
    };
  }, [active, hideNativeSurface, nodeId, reportRuntimeFailure, runtime]);

  useLayoutEffect(() => {
    scheduleGeometryRef.current();
  });

  function submitAddress(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const navigationUrl = normalizeBrowserNavigationUrl(address);
    if (!navigationUrl) {
      setError(INVALID_ADDRESS_MESSAGE);
      return;
    }
    const persistableUrl = normalizeBrowserUrl(navigationUrl);

    setError(null);
    setAddress(navigationUrl);
    setCurrentUrl(navigationUrl);
    currentUrlRef.current = navigationUrl;
    pendingPersistedUrlRef.current = persistableUrl;
    void callIntegrationCallback(
      () => callbacksRef.current.onNavigate(persistableUrl),
      reportRuntimeFailure,
    );
    if (active && runtimeAvailable) {
      void ensureNativeOpen(navigationUrl).then(() => {
        if (!activeRef.current || !openedRef.current) return;
        return runtime.focus({ nodeId }).catch(reportRuntimeFailure);
      });
    }
  }

  function openExternal(): void {
    const safeUrl = currentUrlRef.current;
    if (!safeUrl) return;
    void callIntegrationCallback(
      () => callbacksRef.current.onOpenExternal(safeUrl),
      reportRuntimeFailure,
    );
  }

  function runNativeAction(
    action: (request: { readonly nodeId: string }) => Promise<void>,
  ): void {
    setError(null);
    void action({ nodeId }).catch(reportRuntimeFailure);
  }

  const classes = className
    ? `browser-surface ${className}`
    : "browser-surface";
  const placeholder = getPlaceholder({
    active,
    visible: visible && geometryAllowsVisibility,
    runtimeAvailable,
    currentUrl,
    loading,
    unavailableReason,
  });

  return (
    <section
      className={classes}
      data-shortcut-scope="true"
      role="region"
      aria-label={accessibleLabel}
    >
      <nav className="browser-surface__chrome" aria-label="Browser controls">
        <div className="browser-surface__history-controls">
          <button
            className="browser-surface__icon-button"
            type="button"
            aria-label="Go back"
            title="Go back"
            disabled={!canControlNative}
            onClick={() => runNativeAction(runtime.goBack)}
          >
            <Icon name="arrow-left" />
          </button>
          <button
            className="browser-surface__icon-button"
            type="button"
            aria-label="Go forward"
            title="Go forward"
            disabled={!canControlNative}
            onClick={() => runNativeAction(runtime.goForward)}
          >
            <Icon name="arrow-right" />
          </button>
          <button
            className="browser-surface__icon-button"
            type="button"
            aria-label="Reload page"
            title="Reload page"
            disabled={!canControlNative}
            onClick={() => runNativeAction(runtime.reload)}
          >
            <Icon name="refresh" />
          </button>
        </div>

        <form className="browser-surface__address-form" onSubmit={submitAddress}>
          <label className="visually-hidden" htmlFor={inputId}>
            Address
          </label>
          <input
            ref={inputRef}
            id={inputId}
            name="browser-address"
            type="text"
            inputMode="url"
            autoCapitalize="none"
            autoComplete="off"
            autoCorrect="off"
            spellCheck={false}
            value={address}
            aria-invalid={error === INVALID_ADDRESS_MESSAGE}
            aria-describedby={surfaceDescriptionId}
            placeholder="https://example.com"
            onChange={(event) => setAddress(event.currentTarget.value)}
            onBlur={(event) => {
              const nextTarget = event.relatedTarget;
              if (
                nextTarget instanceof Node &&
                event.currentTarget.form?.contains(nextTarget)
              ) {
                return;
              }
              setAddress(currentUrlRef.current);
            }}
          />
          <button className="browser-surface__go" type="submit">
            Go
          </button>
        </form>

        <button
          className="browser-surface__icon-button"
          type="button"
          aria-label="Open in default browser"
          title="Open in default browser"
          disabled={!currentUrl || !runtimeAvailable}
          onClick={openExternal}
        >
          <Icon name="external-link" />
        </button>
        <button
          className="browser-surface__activate"
          type="button"
          aria-label={
            active
              ? "Focus web page; press Escape to return to browser controls"
              : "Activate browser"
          }
          aria-pressed={active}
          title={active ? "Focus page · Escape returns to controls" : undefined}
          disabled={active && !canControlNative}
          onClick={
            active
              ? () => runNativeAction(runtime.focus)
              : onActivate
          }
        >
          {active ? "Focus page" : "Activate browser"}
        </button>
      </nav>

      <div className="browser-surface__status" id={surfaceDescriptionId}>
        {loading ? (
          <output aria-live="polite">Loading page…</output>
        ) : null}
        {error ? <p role="alert">{error}</p> : null}
      </div>

      <div
        ref={surfaceRef}
        className="browser-surface__viewport"
        data-browser-surface-node-id={nodeId}
        data-native-browser-visible={
          active &&
          visible &&
          geometryAllowsVisibility &&
          runtimeAvailable &&
          currentUrl
            ? "true"
            : "false"
        }
        role="region"
        aria-label="Web page"
        aria-busy={loading}
      >
        <p className={placeholder.visuallyHidden ? "visually-hidden" : undefined}>
          {placeholder.message}
        </p>
      </div>
    </section>
  );
}

interface PlaceholderInput {
  readonly active: boolean;
  readonly visible: boolean;
  readonly runtimeAvailable: boolean;
  readonly currentUrl: string;
  readonly loading: boolean;
  readonly unavailableReason?: string;
}

function getPlaceholder({
  active,
  visible,
  runtimeAvailable,
  currentUrl,
  loading,
  unavailableReason,
}: PlaceholderInput): { readonly message: string; readonly visuallyHidden: boolean } {
  if (!active) {
    return {
      message: "Select this card to activate the integrated browser.",
      visuallyHidden: false,
    };
  }
  if (!runtimeAvailable) {
    return {
      message:
        "The integrated preview is available only in the desktop app.",
      visuallyHidden: false,
    };
  }
  if (!visible) {
    return {
      message:
        unavailableReason ??
        "Move the browser fully into view and close overlapping controls to show the page.",
      visuallyHidden: false,
    };
  }
  if (!currentUrl) {
    return {
      message: "Enter an address to open a page.",
      visuallyHidden: false,
    };
  }
  return {
    message: loading
      ? "The native web page is loading."
      : "The native web page is active.",
    visuallyHidden: true,
  };
}

interface BrowserGeometry {
  readonly bounds: BrowserBounds;
  readonly allowsVisibility: boolean;
}

function readBrowserGeometry(
  element: HTMLElement | null,
): BrowserGeometry | null {
  if (!element) return null;
  const rect = element.getBoundingClientRect();
  if (
    !Number.isFinite(rect.x) ||
    !Number.isFinite(rect.y) ||
    !Number.isFinite(rect.width) ||
    !Number.isFinite(rect.height) ||
    rect.width <= 0 ||
    rect.height <= 0
  ) {
    return null;
  }
  const bounds = {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
  };
  const view = element.ownerDocument.defaultView;
  const viewport = element.closest<HTMLElement>("[data-browser-viewport]");
  const viewportRect = viewport?.getBoundingClientRect();
  const owningCanvasNode = element.closest<HTMLElement>(".canvas-node");
  const withinWindow =
    rect.left >= 0 &&
    rect.top >= 0 &&
    (!view ||
      (rect.right <= view.innerWidth && rect.bottom <= view.innerHeight));
  const withinViewport =
    !viewportRect ||
    (viewportRect.width > 0 &&
      viewportRect.height > 0 &&
      rect.left >= viewportRect.left &&
      rect.top >= viewportRect.top &&
      rect.right <= viewportRect.right &&
      rect.bottom <= viewportRect.bottom);
  const obstructed = [
    ...element.ownerDocument.querySelectorAll<HTMLElement>(
      "[data-browser-obstruction], .canvas-node",
    ),
  ].some(
    (obstruction) =>
      obstruction !== owningCanvasNode &&
      !element.contains(obstruction) &&
      rectanglesOverlap(rect, obstruction.getBoundingClientRect()),
  );
  const blockingDialog = Boolean(
    element.ownerDocument.querySelector(
      "dialog[open], [role='dialog'][aria-modal='true']",
    ),
  );
  return {
    bounds,
    allowsVisibility:
      withinWindow && withinViewport && !obstructed && !blockingDialog,
  };
}

function rectanglesOverlap(first: DOMRect, second: DOMRect): boolean {
  return (
    first.width > 0 &&
    first.height > 0 &&
    second.width > 0 &&
    second.height > 0 &&
    first.left < second.right &&
    first.right > second.left &&
    first.top < second.bottom &&
    first.bottom > second.top
  );
}

function requestFrame(
  view: Window | null,
  callback: FrameRequestCallback,
): number {
  if (view?.requestAnimationFrame) return view.requestAnimationFrame(callback);
  return globalThis.setTimeout(() => callback(performance.now()), 16);
}

function cancelFrame(view: Window | null, frameId: number): void {
  if (view?.cancelAnimationFrame) {
    view.cancelAnimationFrame(frameId);
    return;
  }
  globalThis.clearTimeout(frameId);
}

async function callIntegrationCallback(
  callback: () => void | Promise<void>,
  onError: (cause: unknown) => void,
): Promise<void> {
  try {
    await callback();
  } catch (cause) {
    onError(cause);
  }
}
