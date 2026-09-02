use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Position, Rect, Size,
    State, Url, Webview, WebviewBuilder, WebviewUrl,
};

const MAIN_WINDOW_LABEL: &str = "main";
const MAIN_WEBVIEW_LABEL: &str = "main";
const SURFACE_LABEL_PREFIX: &str = "browser-surface-";
const FOCUS_MAIN_URL: &str = "jig-focus://main/";
const MAX_NODE_ID_LENGTH: usize = 128;
const MAX_URL_LENGTH: usize = 2_048;
const MIN_COORDINATE: f64 = -32_768.0;
const MAX_COORDINATE: f64 = 32_768.0;
const MIN_DIMENSION: f64 = 1.0;
const MAX_DIMENSION: f64 = 8_192.0;

// Tauri 2.11.5 uses wry 0.55, which does not expose a native permission
// handler and grants WKWebView media requests before macOS applies its own
// checks. Keep this document-start guard until Tauri exposes that handler.
const DENY_REMOTE_PERMISSIONS_SCRIPT: &str = r"
(() => {
  'use strict';
  const notAllowed = () => {
    if (typeof DOMException === 'function') {
      return new DOMException('Permission denied by Jig.', 'NotAllowedError');
    }
    const error = new Error('Permission denied by Jig.');
    error.name = 'NotAllowedError';
    return error;
  };
  const denyPromise = () => Promise.reject(notAllowed());
  const define = (target, name, value) => {
    if (!target) return;
    try {
      Object.defineProperty(target, name, {
        configurable: false,
        enumerable: false,
        value,
        writable: false,
      });
    } catch (_) {}
  };

  if (typeof MediaDevices !== 'undefined') {
    define(MediaDevices.prototype, 'getUserMedia', denyPromise);
    define(MediaDevices.prototype, 'getDisplayMedia', denyPromise);
  }
  if (typeof navigator !== 'undefined') {
    if (navigator.mediaDevices) {
      define(navigator.mediaDevices, 'getUserMedia', denyPromise);
      define(navigator.mediaDevices, 'getDisplayMedia', denyPromise);
    }
    const denyLegacyMedia = (_constraints, _success, failure) => {
      if (typeof failure === 'function') queueMicrotask(() => failure(notAllowed()));
    };
    define(navigator, 'getUserMedia', denyLegacyMedia);
    define(navigator, 'webkitGetUserMedia', denyLegacyMedia);
  }
  if (typeof Geolocation !== 'undefined') {
    const denyGeolocation = (_success, failure) => {
      if (typeof failure === 'function') {
        queueMicrotask(() => failure({ code: 1, message: 'Permission denied by Jig.' }));
      }
      return 0;
    };
    define(Geolocation.prototype, 'getCurrentPosition', denyGeolocation);
    define(Geolocation.prototype, 'watchPosition', denyGeolocation);
    if (typeof navigator !== 'undefined' && navigator.geolocation) {
      define(navigator.geolocation, 'getCurrentPosition', denyGeolocation);
      define(navigator.geolocation, 'watchPosition', denyGeolocation);
    }
  }
  if (typeof Notification !== 'undefined') {
    define(Notification, 'requestPermission', () => Promise.resolve('denied'));
  }
  window.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopImmediatePropagation();
    window.location.assign('jig-focus://main/');
  }, true);
})();
";

#[derive(Clone)]
struct ActiveSurface {
    node_id: String,
    label: String,
    generation: u64,
}

/// Owns the single remote child webview allowed inside the main window.
pub(crate) struct BrowserSurfaceHost {
    active: Mutex<Option<ActiveSurface>>,
    generation: Arc<AtomicU64>,
}

impl Default for BrowserSurfaceHost {
    fn default() -> Self {
        Self {
            active: Mutex::new(None),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl BrowserSurfaceHost {
    fn lock(&self) -> Result<MutexGuard<'_, Option<ActiveSurface>>, BrowserSurfaceError> {
        self.active
            .lock()
            .map_err(|_| BrowserSurfaceError::internal())
    }

    fn next_generation(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl BrowserBounds {
    fn validate(self) -> Result<Self, BrowserSurfaceError> {
        let position_is_valid = self.x.is_finite()
            && self.y.is_finite()
            && (MIN_COORDINATE..=MAX_COORDINATE).contains(&self.x)
            && (MIN_COORDINATE..=MAX_COORDINATE).contains(&self.y);
        let size_is_valid = self.width.is_finite()
            && self.height.is_finite()
            && (MIN_DIMENSION..=MAX_DIMENSION).contains(&self.width)
            && (MIN_DIMENSION..=MAX_DIMENSION).contains(&self.height);

        if position_is_valid && size_is_valid {
            Ok(self)
        } else {
            Err(BrowserSurfaceError::invalid_bounds())
        }
    }

    fn into_rect(self) -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(self.x, self.y)),
            size: Size::Logical(LogicalSize::new(self.width, self.height)),
        }
    }

    fn validate_inside(
        self,
        viewport_width: f64,
        viewport_height: f64,
    ) -> Result<Self, BrowserSurfaceError> {
        let right = self.x + self.width;
        let bottom = self.y + self.height;
        if self.x >= 0.0
            && self.y >= 0.0
            && right.is_finite()
            && bottom.is_finite()
            && right <= viewport_width
            && bottom <= viewport_height
        {
            Ok(self)
        } else {
            Err(BrowserSurfaceError::invalid_bounds())
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserSurfaceOpenRequest {
    node_id: String,
    url: String,
    bounds: BrowserBounds,
    visible: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserSurfaceNavigateRequest {
    node_id: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserSurfaceUpdateRequest {
    node_id: String,
    bounds: BrowserBounds,
    visible: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserSurfaceNodeRequest {
    node_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserLocationChanged {
    node_id: String,
    url: String,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum BrowserLoadStatus {
    Started,
    Finished,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserLoadState {
    node_id: String,
    status: BrowserLoadStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserSurfaceError {
    code: &'static str,
    message: &'static str,
}

impl BrowserSurfaceError {
    const fn forbidden() -> Self {
        Self {
            code: "browser_surface_forbidden",
            message: "Only the main application webview may control the browser surface.",
        }
    }

    const fn invalid_node_id() -> Self {
        Self {
            code: "browser_surface_invalid_node",
            message: "The browser node identifier is invalid.",
        }
    }

    const fn invalid_url() -> Self {
        Self {
            code: "browser_surface_invalid_url",
            message: "The browser address must be a permitted HTTP or HTTPS URL.",
        }
    }

    const fn invalid_bounds() -> Self {
        Self {
            code: "browser_surface_invalid_bounds",
            message: "The browser surface bounds are invalid.",
        }
    }

    const fn not_active() -> Self {
        Self {
            code: "browser_surface_not_active",
            message: "The requested browser node does not own the active surface.",
        }
    }

    const fn unavailable() -> Self {
        Self {
            code: "browser_surface_unavailable",
            message: "The native browser surface is unavailable.",
        }
    }

    const fn internal() -> Self {
        Self {
            code: "browser_surface_internal",
            message: "The native browser surface state is unavailable.",
        }
    }
}

/// Returns whether an IPC command originated in the bundled main webview.
pub(crate) fn is_main_webview(caller: &Webview) -> bool {
    caller.label() == MAIN_WEBVIEW_LABEL && caller.window().label() == MAIN_WINDOW_LABEL
}

#[tauri::command]
pub(crate) async fn browser_surface_open(
    request: BrowserSurfaceOpenRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(&caller)?;
    validate_node_id(&request.node_id)?;
    let bounds = validate_bounds_or_hide(&caller, request.bounds, request.visible)?;
    let dev_origin = app.config().build.dev_url.as_ref();
    let url = parse_browser_url(&request.url, dev_origin)?;
    let mut active = host.lock()?;

    if let Some(current) = active.clone() {
        if current.generation == host.generation.load(Ordering::Acquire) {
            if let Some(webview) = app.get_webview(&current.label) {
                if current.node_id == request.node_id {
                    let current_url = webview
                        .url()
                        .map_err(|_| BrowserSurfaceError::unavailable())?;
                    if current_url != url {
                        webview
                            .navigate(url)
                            .map_err(|_| BrowserSurfaceError::unavailable())?;
                    }
                    apply_bounds_and_visibility(&webview, bounds, request.visible)?;
                    return Ok(());
                }

                close_surface(&webview, &host)?;
            }
        }
        *active = None;
        host.invalidate();
    }

    host.invalidate();
    close_orphan_surfaces(&caller)?;

    let generation = host.next_generation();
    let label = browser_surface_label(generation);
    let builder = browser_builder(
        &app,
        label.clone(),
        request.node_id.clone(),
        url,
        generation,
        Arc::clone(&host.generation),
    );
    let initial_bounds = if request.visible {
        bounds
    } else {
        BrowserBounds {
            x: 0.0,
            y: 0.0,
            width: MIN_DIMENSION,
            height: MIN_DIMENSION,
        }
    };
    let webview = caller
        .window()
        .add_child(
            builder,
            LogicalPosition::new(initial_bounds.x, initial_bounds.y),
            LogicalSize::new(initial_bounds.width, initial_bounds.height),
        )
        .map_err(|_| {
            host.invalidate();
            BrowserSurfaceError::unavailable()
        })?;

    if let Err(error) = apply_bounds_and_visibility(&webview, bounds, request.visible) {
        host.invalidate();
        let _ = webview.close();
        return Err(error);
    }

    *active = Some(ActiveSurface {
        node_id: request.node_id,
        label,
        generation,
    });
    Ok(())
}

#[tauri::command]
pub(crate) async fn browser_surface_navigate(
    request: BrowserSurfaceNavigateRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(&caller)?;
    validate_node_id(&request.node_id)?;
    let url = parse_browser_url(&request.url, app.config().build.dev_url.as_ref())?;
    let active = host.lock()?;
    let webview = active_webview(&app, &host, active.as_ref(), &request.node_id)?;
    webview
        .navigate(url)
        .map_err(|_| BrowserSurfaceError::unavailable())
}

#[tauri::command]
pub(crate) async fn browser_surface_update(
    request: BrowserSurfaceUpdateRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(&caller)?;
    validate_node_id(&request.node_id)?;
    let bounds = validate_bounds_or_hide(&caller, request.bounds, request.visible)?;
    let active = host.lock()?;
    let webview = active_webview(&app, &host, active.as_ref(), &request.node_id)?;
    apply_bounds_and_visibility(&webview, bounds, request.visible)
}

#[tauri::command]
pub(crate) async fn browser_surface_reload(
    request: BrowserSurfaceNodeRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(&caller)?;
    validate_node_id(&request.node_id)?;
    let active = host.lock()?;
    let webview = active_webview(&app, &host, active.as_ref(), &request.node_id)?;
    webview
        .reload()
        .map_err(|_| BrowserSurfaceError::unavailable())
}

#[tauri::command]
pub(crate) async fn browser_surface_go_back(
    request: BrowserSurfaceNodeRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    evaluate_history_action(&request, &caller, &app, &host, "history.back()")
}

#[tauri::command]
pub(crate) async fn browser_surface_go_forward(
    request: BrowserSurfaceNodeRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    evaluate_history_action(&request, &caller, &app, &host, "history.forward()")
}

#[tauri::command]
pub(crate) async fn browser_surface_focus(
    request: BrowserSurfaceNodeRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(&caller)?;
    validate_node_id(&request.node_id)?;
    let active = host.lock()?;
    let webview = active_webview(&app, &host, active.as_ref(), &request.node_id)?;
    webview
        .set_focus()
        .map_err(|_| BrowserSurfaceError::unavailable())
}

#[tauri::command]
pub(crate) async fn browser_surface_close(
    request: BrowserSurfaceNodeRequest,
    caller: Webview,
    app: AppHandle,
    host: State<'_, BrowserSurfaceHost>,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(&caller)?;
    validate_node_id(&request.node_id)?;
    let mut active = host.lock()?;
    let Some(current) = active.as_ref() else {
        return Ok(());
    };
    if current.node_id != request.node_id {
        return Ok(());
    }

    if let Some(webview) = app.get_webview(&current.label) {
        close_surface(&webview, &host)?;
    } else {
        host.invalidate();
    }
    *active = None;
    Ok(())
}

fn require_main_caller(caller: &Webview) -> Result<(), BrowserSurfaceError> {
    if is_main_webview(caller) {
        Ok(())
    } else {
        Err(BrowserSurfaceError::forbidden())
    }
}

fn browser_builder(
    app: &AppHandle,
    label: String,
    node_id: String,
    url: Url,
    generation: u64,
    active_generation: Arc<AtomicU64>,
) -> WebviewBuilder<tauri::Wry> {
    let navigation_dev_origin = app.config().build.dev_url.clone();
    let page_dev_origin = navigation_dev_origin.clone();
    let navigation_generation = Arc::clone(&active_generation);
    let navigation_app = app.clone();
    let page_generation = active_generation;
    let event_app = app.clone();
    let location_node_id = node_id.clone();

    let builder = WebviewBuilder::new(label, WebviewUrl::External(url))
        .initialization_script_for_all_frames(DENY_REMOTE_PERMISSIONS_SCRIPT)
        .incognito(true)
        .focused(false)
        .devtools(false)
        .on_navigation(move |candidate| {
            let is_current = navigation_generation.load(Ordering::Acquire) == generation;
            if is_current && is_focus_main_url(candidate) {
                if let Some(main) = navigation_app.get_webview(MAIN_WEBVIEW_LABEL) {
                    let _ = main.set_focus();
                }
                return false;
            }
            is_current
                && validate_parsed_browser_url(candidate, navigation_dev_origin.as_ref()).is_ok()
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_page_load(move |webview, payload| {
            if page_generation.load(Ordering::Acquire) != generation {
                return;
            }
            if validate_parsed_browser_url(payload.url(), page_dev_origin.as_ref()).is_err() {
                page_generation.fetch_add(1, Ordering::AcqRel);
                let _ = webview.close();
                return;
            }

            let status = match payload.event() {
                PageLoadEvent::Started => BrowserLoadStatus::Started,
                PageLoadEvent::Finished => BrowserLoadStatus::Finished,
            };
            if matches!(status, BrowserLoadStatus::Started) {
                let _ = event_app.emit_to(
                    EventTarget::webview(MAIN_WEBVIEW_LABEL),
                    "browser:location-changed",
                    BrowserLocationChanged {
                        node_id: location_node_id.clone(),
                        url: sanitize_location_url(payload.url()),
                    },
                );
            }
            let _ = event_app.emit_to(
                EventTarget::webview(MAIN_WEBVIEW_LABEL),
                "browser:load-state",
                BrowserLoadState {
                    node_id: node_id.clone(),
                    status,
                },
            );
        });

    #[cfg(target_os = "macos")]
    let builder = builder.allow_link_preview(false);

    builder
}

fn apply_bounds_and_visibility(
    webview: &Webview,
    bounds: BrowserBounds,
    visible: bool,
) -> Result<(), BrowserSurfaceError> {
    // A child webview does not follow DOM clipping or stacking. Hide it before
    // moving it so a failed bounds update cannot leave remote content visible
    // over trusted application controls at its previous position.
    webview
        .hide()
        .map_err(|_| BrowserSurfaceError::unavailable())?;
    webview
        .set_bounds(bounds.into_rect())
        .map_err(|_| BrowserSurfaceError::unavailable())?;
    if visible {
        webview
            .show()
            .map_err(|_| BrowserSurfaceError::unavailable())?;
    }
    Ok(())
}

fn close_surface(webview: &Webview, host: &BrowserSurfaceHost) -> Result<(), BrowserSurfaceError> {
    let _ = webview.hide();
    host.invalidate();
    webview
        .close()
        .map_err(|_| BrowserSurfaceError::unavailable())
}

fn validate_visible_bounds(
    caller: &Webview,
    bounds: BrowserBounds,
    visible: bool,
) -> Result<(), BrowserSurfaceError> {
    if !visible {
        return Ok(());
    }
    let window = caller.window();
    let scale_factor = window
        .scale_factor()
        .map_err(|_| BrowserSurfaceError::unavailable())?;
    let viewport = window
        .inner_size()
        .map_err(|_| BrowserSurfaceError::unavailable())?
        .to_logical::<f64>(scale_factor);
    bounds
        .validate_inside(viewport.width, viewport.height)
        .map(|_| ())
}

fn validate_bounds_or_hide(
    caller: &Webview,
    bounds: BrowserBounds,
    visible: bool,
) -> Result<BrowserBounds, BrowserSurfaceError> {
    let validation = bounds
        .validate()
        .and_then(|bounds| validate_visible_bounds(caller, bounds, visible).map(|()| bounds));
    match validation {
        Ok(bounds) => Ok(bounds),
        Err(error) => {
            for webview in caller.window().webviews() {
                if is_browser_surface_label(webview.label()) && webview.hide().is_err() {
                    let _ = webview.close();
                }
            }
            Err(error)
        }
    }
}

fn active_webview(
    app: &AppHandle,
    host: &BrowserSurfaceHost,
    active: Option<&ActiveSurface>,
    node_id: &str,
) -> Result<Webview, BrowserSurfaceError> {
    let current = active.ok_or_else(BrowserSurfaceError::not_active)?;
    if current.node_id != node_id || current.generation != host.generation.load(Ordering::Acquire) {
        return Err(BrowserSurfaceError::not_active());
    }
    app.get_webview(&current.label)
        .ok_or_else(BrowserSurfaceError::not_active)
}

fn evaluate_history_action(
    request: &BrowserSurfaceNodeRequest,
    caller: &Webview,
    app: &AppHandle,
    host: &BrowserSurfaceHost,
    script: &'static str,
) -> Result<(), BrowserSurfaceError> {
    require_main_caller(caller)?;
    validate_node_id(&request.node_id)?;
    let active = host.lock()?;
    let webview = active_webview(app, host, active.as_ref(), &request.node_id)?;
    webview
        .eval(script)
        .map_err(|_| BrowserSurfaceError::unavailable())
}

fn close_orphan_surfaces(caller: &Webview) -> Result<(), BrowserSurfaceError> {
    for webview in caller.window().webviews() {
        if is_browser_surface_label(webview.label()) {
            let _ = webview.hide();
            webview
                .close()
                .map_err(|_| BrowserSurfaceError::unavailable())?;
        }
    }
    Ok(())
}

fn validate_node_id(node_id: &str) -> Result<(), BrowserSurfaceError> {
    let valid = !node_id.is_empty()
        && node_id.len() <= MAX_NODE_ID_LENGTH
        && node_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(BrowserSurfaceError::invalid_node_id())
    }
}

fn browser_surface_label(generation: u64) -> String {
    format!("{SURFACE_LABEL_PREFIX}{generation:016x}")
}

fn is_browser_surface_label(label: &str) -> bool {
    label.starts_with(SURFACE_LABEL_PREFIX)
}

fn parse_browser_url(value: &str, dev_origin: Option<&Url>) -> Result<Url, BrowserSurfaceError> {
    if value.is_empty()
        || value.len() > MAX_URL_LENGTH
        || value.trim() != value
        || value.chars().any(char::is_control)
        || contains_raw_userinfo(value)
    {
        return Err(BrowserSurfaceError::invalid_url());
    }
    let url = Url::parse(value).map_err(|_| BrowserSurfaceError::invalid_url())?;
    validate_parsed_browser_url(&url, dev_origin)?;
    Ok(url)
}

fn validate_parsed_browser_url(
    url: &Url,
    dev_origin: Option<&Url>,
) -> Result<(), BrowserSurfaceError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || contains_userinfo(url)
        || url.as_str().len() > MAX_URL_LENGTH
        || is_internal_protocol_host(url)
        || dev_origin.is_some_and(|origin| same_origin(url, origin))
        || is_default_dev_origin(url)
    {
        return Err(BrowserSurfaceError::invalid_url());
    }
    Ok(())
}

fn contains_userinfo(url: &Url) -> bool {
    !url.username().is_empty() || url.password().is_some() || contains_raw_userinfo(url.as_str())
}

fn contains_raw_userinfo(value: &str) -> bool {
    value
        .split_once("://")
        .and_then(|(_, rest)| rest.split(['/', '?', '#']).next())
        .is_some_and(|authority| authority.contains('@'))
}

fn is_internal_protocol_host(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        let host = host.trim_end_matches('.');
        host.eq_ignore_ascii_case("tauri.localhost")
            || host.eq_ignore_ascii_case("ipc.localhost")
            || host.eq_ignore_ascii_case("asset.localhost")
    })
}

fn is_focus_main_url(url: &Url) -> bool {
    url.as_str() == FOCUS_MAIN_URL
}

fn is_default_dev_origin(url: &Url) -> bool {
    url.port_or_known_default() == Some(1_420) && url.host_str().is_some_and(is_loopback_host)
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    normalized.eq_ignore_ascii_case("localhost")
        || normalized
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left
            .host_str()
            .zip(right.host_str())
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn sanitize_location_url(url: &Url) -> String {
    let mut sanitized = url.clone();
    sanitized.set_fragment(None);

    let retained_pairs = url
        .query_pairs()
        .filter(|(name, _)| !is_sensitive_query_parameter(name))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    sanitized.set_query(None);
    if !retained_pairs.is_empty() {
        let mut query = sanitized.query_pairs_mut();
        for (name, value) in retained_pairs {
            query.append_pair(&name, &value);
        }
    }

    sanitized.to_string()
}

fn is_sensitive_query_parameter(name: &str) -> bool {
    let normalized = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized.starts_with("xamz")
        || normalized.ends_with("token")
        || matches!(
            normalized.as_str(),
            "token"
                | "accesstoken"
                | "idtoken"
                | "code"
                | "key"
                | "apikey"
                | "secret"
                | "clientsecret"
                | "refreshtoken"
                | "sig"
                | "signature"
                | "credential"
                | "credentials"
                | "auth"
                | "authorization"
                | "password"
                | "passwd"
                | "pwd"
                | "session"
                | "sessionid"
                | "state"
                | "samlresponse"
                | "policy"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_origin() -> Url {
        Url::parse("http://localhost:1420").expect("valid test dev origin")
    }

    #[test]
    fn accepts_only_external_http_and_https_urls() {
        let dev_origin = dev_origin();

        assert_eq!(
            parse_browser_url("https://example.com/docs?q=rust#api", Some(&dev_origin))
                .expect("https should be accepted")
                .as_str(),
            "https://example.com/docs?q=rust#api"
        );
        assert_eq!(
            parse_browser_url("http://localhost:3000/", Some(&dev_origin))
                .expect("non-app localhost should be accepted")
                .as_str(),
            "http://localhost:3000/"
        );

        for value in [
            "tauri://localhost/",
            "file:///tmp/private",
            "data:text/html,hello",
            "javascript:alert(1)",
            "about:blank",
            "ws://example.com/socket",
            "https://user:password@example.com/",
            "https://@example.com/",
            "https://tauri.localhost/",
            "http://localhost:1420/",
            "http://127.0.0.1:1420/",
            "http://[::1]:1420/",
            " https://example.com/",
        ] {
            assert!(
                parse_browser_url(value, Some(&dev_origin)).is_err(),
                "URL should be rejected"
            );
        }
    }

    #[test]
    fn rejects_configured_dev_origin_regardless_of_path() {
        let dev_origin = Url::parse("https://dev.jig.test:7443/app").expect("valid test origin");

        assert!(parse_browser_url("https://dev.jig.test:7443/other", Some(&dev_origin)).is_err());
        assert!(parse_browser_url("https://dev.jig.test/", Some(&dev_origin)).is_ok());
    }

    #[test]
    fn creates_bounded_deterministic_webview_labels() {
        let first = browser_surface_label(1);
        let repeated = browser_surface_label(1);
        let second = browser_surface_label(2);

        assert_eq!(first, repeated);
        assert_ne!(first, second);
        assert!(is_browser_surface_label(&first));
        assert!(first.len() <= SURFACE_LABEL_PREFIX.len() + 16);
        assert!(validate_node_id("").is_err());
        assert!(validate_node_id(&"x".repeat(MAX_NODE_ID_LENGTH + 1)).is_err());
        assert!(validate_node_id("browser/invalid").is_err());
    }

    #[test]
    fn validates_finite_bounded_surface_geometry() {
        assert!(
            BrowserBounds {
                x: -120.5,
                y: 48.0,
                width: 640.0,
                height: 420.0,
            }
            .validate()
            .is_ok()
        );

        for bounds in [
            BrowserBounds {
                x: f64::NAN,
                y: 0.0,
                width: 640.0,
                height: 420.0,
            },
            BrowserBounds {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 420.0,
            },
            BrowserBounds {
                x: 0.0,
                y: 0.0,
                width: MAX_DIMENSION + 1.0,
                height: 420.0,
            },
            BrowserBounds {
                x: MAX_COORDINATE + 1.0,
                y: 0.0,
                width: 640.0,
                height: 420.0,
            },
        ] {
            assert!(bounds.validate().is_err());
        }

        let bounds = BrowserBounds {
            x: 120.0,
            y: 80.0,
            width: 640.0,
            height: 420.0,
        };
        assert!(bounds.validate_inside(1_280.0, 800.0).is_ok());
        assert!(bounds.validate_inside(700.0, 800.0).is_err());
        assert!(
            BrowserBounds { x: -1.0, ..bounds }
                .validate_inside(1_280.0, 800.0)
                .is_err()
        );
    }

    #[test]
    fn redacts_secrets_and_fragments_from_location_events() {
        let url = Url::parse(
            "https://example.com/callback?tab=activity&access_token=secret&refresh_token=refresh&oauth_token=legacy&state=csrf&SAMLResponse=assertion&Policy=cloudfront&X-Amz-Signature=signed&code=oauth#private",
        )
        .expect("valid test URL");

        assert_eq!(
            sanitize_location_url(&url),
            "https://example.com/callback?tab=activity"
        );
        assert_eq!(
            sanitize_location_url(
                &Url::parse("https://example.com/path?q=rust&page=2#heading")
                    .expect("valid test URL")
            ),
            "https://example.com/path?q=rust&page=2"
        );
    }

    #[test]
    fn remote_permission_guard_denies_sensitive_browser_apis() {
        for api in [
            "getUserMedia",
            "getDisplayMedia",
            "getCurrentPosition",
            "watchPosition",
            "requestPermission",
        ] {
            assert!(DENY_REMOTE_PERMISSIONS_SCRIPT.contains(api));
        }
        assert!(DENY_REMOTE_PERMISSIONS_SCRIPT.contains("configurable: false"));
        assert!(DENY_REMOTE_PERMISSIONS_SCRIPT.contains("NotAllowedError"));
        assert!(DENY_REMOTE_PERMISSIONS_SCRIPT.contains("event.key !== 'Escape'"));
        assert!(DENY_REMOTE_PERMISSIONS_SCRIPT.contains(FOCUS_MAIN_URL));
        assert!(is_focus_main_url(
            &Url::parse(FOCUS_MAIN_URL).expect("valid internal focus URL")
        ));
    }
}
