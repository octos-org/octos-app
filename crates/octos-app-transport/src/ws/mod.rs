//! WebSocket transport task. Owns the `tokio-tungstenite` socket and runs
//! the connection state machine (`Idle → Dialing → Handshaking → Live ↔
//! Reconnecting → Failed`). The inner `select!` arbitrates outbound commands,
//! inbound frames, and 30-s heartbeat ticks; reconnect uses W01's full-jitter
//! backoff with a 5-min cumulative budget.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use octos_core::app_ui::AppUiBackendEvent as UiNotification;
use octos_core::ui_protocol::{
    methods, ApprovalRespondResult, DiffPreviewGetResult, RpcError, TaskOutputReadResult,
    UiCursor, UiRpcResult,
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::handshake::client::Request as WsRequest;
use tokio_tungstenite::tungstenite::http::Uri as WsUri;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::capability::Capabilities;
use crate::jsonrpc::{serialize_request, JsonRpcId, RpcEnvelope, RpcRegistry};
use crate::{
    ConnectionState, LifecycleResult, OutboundCommand, ProfileId, SecretString,
    TransportConfig, TransportEvent,
};

pub const CHANNEL_BUFFER: usize = 64;
pub const RECONNECT_DELAY_MAX: Duration = Duration::from_secs(30);
pub const RECONNECT_BUDGET: Duration = Duration::from_secs(5 * 60);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

pub struct WsTransport {
    pub(crate) join: tokio::task::JoinHandle<()>,
}

impl WsTransport {
    pub fn abort(self) {
        self.join.abort();
    }
}

pub fn spawn(
    cfg: TransportConfig,
) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(CHANNEL_BUFFER);
    let (evt_tx, evt_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
    tokio::spawn(async move { run_state_machine(cfg, cmd_rx, evt_tx).await });
    (cmd_tx, evt_rx)
}

struct PendingRequest {
    method: &'static str,
    reply: PendingReply,
}

enum PendingReply {
    Lifecycle,
    Approval(oneshot::Sender<Result<ApprovalRespondResult, RpcError>>),
    DiffPreview(oneshot::Sender<Result<DiffPreviewGetResult, RpcError>>),
    TaskOutput(oneshot::Sender<Result<TaskOutputReadResult, RpcError>>),
    /// `session/list` — result re-emitted as `TransportEvent::SessionsListed`.
    SessionList,
}

struct SharedState {
    cursor: Option<UiCursor>,
    pending: HashMap<JsonRpcId, PendingRequest>,
    registry: Arc<RpcRegistry>,
}

impl SharedState {
    fn new(cursor: Option<UiCursor>) -> Self {
        Self {
            cursor,
            pending: HashMap::new(),
            registry: Arc::new(RpcRegistry::new()),
        }
    }
}

fn build_ws_uri(base: &url::Url) -> Result<WsUri, String> {
    let mut url = base.clone();
    let scheme = match url.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        other => return Err(format!("unsupported scheme: {other}")),
    };
    url.set_scheme(scheme).map_err(|_| "set_scheme failed".to_owned())?;
    url.path_segments_mut()
        .map_err(|_| "cannot-be-a-base url".to_owned())?
        .pop_if_empty()
        .extend(["api", "ui-protocol", "ws"]);
    url.as_str().parse::<WsUri>().map_err(|e| format!("uri parse: {e}"))
}

fn build_request(
    base: &url::Url,
    token: &SecretString,
    profile: &ProfileId,
    requested_capabilities: &Capabilities,
) -> Result<WsRequest, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let uri = build_ws_uri(base)?;
    let mut req = uri.into_client_request().map_err(|e| format!("into_client_request: {e}"))?;
    let h = req.headers_mut();
    h.insert(
        "authorization",
        format!("Bearer {}", token.expose())
            .parse()
            .map_err(|e| format!("auth header: {e}"))?,
    );
    h.insert(
        "x-profile-id",
        profile.0.parse().map_err(|e| format!("profile header: {e}"))?,
    );
    if let Some(features) = requested_capabilities.handshake_header_value() {
        h.insert(
            "x-octos-ui-features",
            features
                .parse()
                .map_err(|e| format!("ui features header: {e}"))?,
        );
    }
    Ok(req)
}

/// `message/delta` is the only ephemeral notification per
/// `03-PROTOCOL-CONTRACT.md` § "Live streaming output".
fn is_ephemeral_method(method: &str) -> bool {
    method == methods::MESSAGE_DELTA
}

/// Try to send an event without blocking. Logs a warning if the receiver
/// can't keep up — backpressure protects the WS read loop from a slow UI.
fn try_emit(events: &mpsc::Sender<TransportEvent>, evt: TransportEvent) {
    match events.try_send(evt) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            log::warn!("transport: event channel full, dropping frame");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}

async fn emit_durable_notification(
    events: &mpsc::Sender<TransportEvent>,
    payload: UiNotification,
    cursor: Option<UiCursor>,
) {
    if events
        .send(TransportEvent::DurableNotification { payload, cursor })
        .await
        .is_err()
    {
        log::debug!("transport: event receiver closed while sending durable notification");
    }
}

async fn run_state_machine(
    cfg: TransportConfig,
    mut commands: mpsc::Receiver<OutboundCommand>,
    events: mpsc::Sender<TransportEvent>,
) {
    let mut shared = SharedState::new(cfg.cursor.clone());
    let mut attempt: u32 = 0;
    let mut total_wait = Duration::ZERO;

    log::info!("ws: state machine up (base_url={})", cfg.base_url);
    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Idle));

    loop {
        log::info!("ws: dialing");
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Dialing));
        let req = match build_request(
            &cfg.base_url,
            &cfg.bearer,
            &cfg.profile_id,
            &cfg.requested_capabilities,
        ) {
            Ok(r) => r,
            Err(e) => {
                log::error!("ws: bad upgrade request: {e}");
                try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
                break;
            }
        };
        let socket = match tokio_tungstenite::connect_async(req).await {
            Ok((s, _)) => s,
            Err(e) => {
                log::warn!("ws: connect failed: {e}");
                if !run_reconnect(&events, &mut attempt, &mut total_wait).await {
                    break;
                }
                continue;
            }
        };
        attempt = 0;
        total_wait = Duration::ZERO;
        log::info!("ws: connected; handshaking");
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Handshaking));
        match run_live(socket, &mut shared, &mut commands, &events).await {
            LiveExit::Disconnect => break,
            LiveExit::Reconnect => {
                if !run_reconnect(&events, &mut attempt, &mut total_wait).await {
                    break;
                }
            }
        }
    }
    shared.registry.cancel_all();
    shared.pending.clear();
    while commands.try_recv().is_ok() {}
}

enum LiveExit {
    Disconnect,
    Reconnect,
}

async fn run_live(
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    shared: &mut SharedState,
    commands: &mut mpsc::Receiver<OutboundCommand>,
    events: &mpsc::Sender<TransportEvent>,
) -> LiveExit {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut state = ConnectionState::Handshaking;

    let mut hb = interval(HEARTBEAT_INTERVAL);
    hb.set_missed_tick_behavior(MissedTickBehavior::Delay);
    hb.tick().await; // skip immediate first tick

    loop {
        tokio::select! {
            biased;
            cmd = commands.recv() => {
                let Some(cmd) = cmd else {
                    let _ = ws_tx.send(WsMessage::Close(None)).await;
                    return LiveExit::Disconnect;
                };
                match handle_command(cmd, &mut ws_tx, shared).await {
                    CommandOutcome::Continue => {}
                    CommandOutcome::Disconnect => {
                        let _ = ws_tx.send(WsMessage::Close(None)).await;
                        return LiveExit::Disconnect;
                    }
                    CommandOutcome::SocketError => return LiveExit::Reconnect,
                }
            }
            frame = ws_rx.next() => {
                let Some(frame) = frame else {
                    log::info!("ws: stream ended");
                    return LiveExit::Reconnect;
                };
                match frame {
                    Ok(WsMessage::Text(text)) => {
                        if let Some(t) = handle_text_frame(&text, shared, events, &mut state).await {
                            try_emit(events, TransportEvent::ConnectionState(t));
                        }
                    }
                    Ok(WsMessage::Binary(_)) => log::warn!("ws: unexpected binary; ignoring"),
                    Ok(WsMessage::Ping(p)) => {
                        if ws_tx.send(WsMessage::Pong(p)).await.is_err() {
                            return LiveExit::Reconnect;
                        }
                    }
                    Ok(WsMessage::Pong(_)) | Ok(WsMessage::Frame(_)) => {}
                    Ok(WsMessage::Close(_)) => return LiveExit::Reconnect,
                    Err(e) => {
                        log::warn!("ws: read error: {e}");
                        return LiveExit::Reconnect;
                    }
                }
            }
            _ = hb.tick() => {
                if ws_tx.send(WsMessage::Ping(Vec::new())).await.is_err() {
                    return LiveExit::Reconnect;
                }
            }
        }
    }
}

enum CommandOutcome {
    Continue,
    Disconnect,
    SocketError,
}

async fn handle_command<S>(
    cmd: OutboundCommand,
    ws_tx: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<S>,
        WsMessage,
    >,
    shared: &mut SharedState,
) -> CommandOutcome
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let id = shared.registry.next_id();
    let (method, body, pending): (&'static str, Value, Option<PendingReply>) = match cmd {
        OutboundCommand::OpenSession(mut params) => {
            // Resume bracket: replay from the last in-memory cursor when one exists.
            if params.after.is_none() {
                params.after = shared.cursor.clone();
            }
            (methods::SESSION_OPEN, to_value(&params), Some(PendingReply::Lifecycle))
        }
        OutboundCommand::StartTurn(p) => (methods::TURN_START, to_value(&p), Some(PendingReply::Lifecycle)),
        OutboundCommand::InterruptTurn(p) => (methods::TURN_INTERRUPT, to_value(&p), Some(PendingReply::Lifecycle)),
        OutboundCommand::SendApprovalResponse { params, reply } => {
            (methods::APPROVAL_RESPOND, to_value(&params), Some(PendingReply::Approval(reply)))
        }
        OutboundCommand::FetchDiffPreview { params, reply } => {
            (methods::DIFF_PREVIEW_GET, to_value(&params), Some(PendingReply::DiffPreview(reply)))
        }
        OutboundCommand::RequestTaskOutput { params, reply } => {
            (methods::TASK_OUTPUT_READ, to_value(&params), Some(PendingReply::TaskOutput(reply)))
        }
        OutboundCommand::ListSessions => (
            methods::SESSION_LIST,
            to_value(&octos_core::ui_protocol::SessionListParams {}),
            Some(PendingReply::SessionList),
        ),
        OutboundCommand::Disconnect => return CommandOutcome::Disconnect,
    };

    let frame = match serialize_request(&id, method, &body) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("ws: serialize {method}: {e}");
            return CommandOutcome::Continue;
        }
    };
    if ws_tx.send(WsMessage::Text(frame)).await.is_err() {
        return CommandOutcome::SocketError;
    }
    if let Some(reply) = pending {
        shared.pending.insert(id, PendingRequest { method, reply });
    }
    CommandOutcome::Continue
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Inbound text-frame dispatcher. Returns `Some(new_state)` if the frame
/// implies a `ConnectionState` transition the outer loop should announce.
async fn handle_text_frame(
    text: &str,
    shared: &mut SharedState,
    events: &mpsc::Sender<TransportEvent>,
    state: &mut ConnectionState,
) -> Option<ConnectionState> {
    let env = match RpcEnvelope::parse(text) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("ws: bad json frame: {e}");
            return None;
        }
    };
    match env {
        RpcEnvelope::Notification(n) => {
            handle_notification(&n.method, n.params, shared, events).await;
            None
        }
        RpcEnvelope::Response(r) => match shared.pending.remove(&r.id) {
            Some(p) => handle_response(p, r.result, events, state),
            None => {
                log::warn!("ws: response for unknown id {}", r.id);
                None
            }
        },
        RpcEnvelope::ErrorResponse(er) => {
            if let Some(id) = er.id.clone() {
                if let Some(pending) = shared.pending.remove(&id) {
                    let method = pending.method.to_owned();
                    fail_pending(pending, er.error.clone());
                    try_emit(
                        events,
                        TransportEvent::RpcError {
                            request_id: id,
                            method,
                            error: er.error,
                        },
                    );
                }
            } else {
                log::warn!("ws: error response missing id: {:?}", er.error);
            }
            None
        }
        RpcEnvelope::Request(req) => {
            log::warn!("ws: server initiated request {} (ignored)", req.method);
            None
        }
    }
}

fn handle_response(
    pending: PendingRequest,
    result_value: Value,
    events: &mpsc::Sender<TransportEvent>,
    state: &mut ConnectionState,
) -> Option<ConnectionState> {
    let method = pending.method;
    match pending.reply {
        PendingReply::Lifecycle => {
            match UiRpcResult::from_method_and_result(method, result_value.clone()) {
                Ok(UiRpcResult::SessionOpen(open)) => {
                    let caps = Capabilities::parse(&result_value);
                    try_emit(events, TransportEvent::CapabilityNegotiated(caps));
                    try_emit(events, TransportEvent::RpcResult(LifecycleResult::SessionOpen(open)));
                    if !matches!(state, ConnectionState::Live) {
                        *state = ConnectionState::Live;
                        return Some(ConnectionState::Live);
                    }
                    None
                }
                Ok(UiRpcResult::TurnStart(r)) => {
                    try_emit(events, TransportEvent::RpcResult(LifecycleResult::TurnStart(r)));
                    None
                }
                Ok(UiRpcResult::TurnInterrupt(r)) => {
                    try_emit(events, TransportEvent::RpcResult(LifecycleResult::TurnInterrupt(r)));
                    None
                }
                Ok(other) => {
                    log::warn!("ws: lifecycle result unexpected variant: {:?}", other.kind());
                    None
                }
                Err(e) => {
                    log::warn!("ws: decode lifecycle result for {method}: {e:?}");
                    None
                }
            }
        }
        PendingReply::Approval(reply) => {
            let _ = reply.send(
                serde_json::from_value::<ApprovalRespondResult>(result_value)
                    .map_err(|e| RpcError::invalid_params(e.to_string())),
            );
            None
        }
        PendingReply::DiffPreview(reply) => {
            let _ = reply.send(
                serde_json::from_value::<DiffPreviewGetResult>(result_value)
                    .map_err(|e| RpcError::invalid_params(e.to_string())),
            );
            None
        }
        PendingReply::TaskOutput(reply) => {
            let _ = reply.send(
                serde_json::from_value::<TaskOutputReadResult>(result_value)
                    .map_err(|e| RpcError::invalid_params(e.to_string())),
            );
            None
        }
        PendingReply::SessionList => {
            match serde_json::from_value::<octos_core::ui_protocol::SessionListResult>(
                result_value,
            ) {
                Ok(r) => try_emit(events, TransportEvent::SessionsListed { sessions: r.sessions }),
                Err(e) => log::warn!("ws: decode session/list result: {e}"),
            }
            None
        }
    }
}

fn fail_pending(pending: PendingRequest, err: RpcError) {
    match pending.reply {
        PendingReply::Lifecycle => {} // surfaced as TransportEvent::RpcError
        PendingReply::Approval(reply) => {
            let _ = reply.send(Err(err));
        }
        PendingReply::DiffPreview(reply) => {
            let _ = reply.send(Err(err));
        }
        PendingReply::TaskOutput(reply) => {
            let _ = reply.send(Err(err));
        }
        // Sidebar hydrate is best-effort; the retry rides the next
        // `session/open` → `CapabilityNegotiated` → `ListSessions` cycle.
        PendingReply::SessionList => {}
    }
}

async fn handle_notification(
    method: &str,
    params: Value,
    shared: &mut SharedState,
    events: &mpsc::Sender<TransportEvent>,
) {
    let payload = match UiNotification::from_method_and_params(method, params.clone()) {
        Ok(p) => p,
        Err(_) => {
            log::warn!("ws: unknown notification method '{method}'");
            return;
        }
    };
    if is_ephemeral_method(method) {
        try_emit(events, TransportEvent::EphemeralNotification { payload });
        return;
    }
    let cursor = params
        .get("cursor")
        .and_then(|v| serde_json::from_value::<UiCursor>(v.clone()).ok());
    if let Some(c) = cursor.clone() {
        shared.cursor = Some(c);
    }
    emit_durable_notification(events, payload, cursor).await;
}

/// Sleep on backoff. Returns `true` to retry, `false` if the cumulative
/// budget is exhausted (caller transitions to `Failed`).
async fn run_reconnect(
    events: &mpsc::Sender<TransportEvent>,
    attempt: &mut u32,
    total_wait: &mut Duration,
) -> bool {
    *attempt = attempt.saturating_add(1);
    let delay = next_backoff(*attempt);
    *total_wait += delay;
    if *total_wait > RECONNECT_BUDGET {
        log::error!("ws: reconnect budget exhausted ({:.1?})", *total_wait);
        try_emit(events, TransportEvent::ConnectionState(ConnectionState::Failed));
        return false;
    }
    try_emit(
        events,
        TransportEvent::ConnectionState(ConnectionState::Reconnecting { attempt: *attempt }),
    );
    let start = Instant::now();
    tokio::time::sleep(delay).await;
    log::debug!("ws: reconnect waited {:.2?} (#{}.)", start.elapsed(), *attempt);
    true
}

/// Full-jitter exponential backoff (W01 § Reconnect algorithm).
pub fn next_backoff(attempt: u32) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let exp = attempt.saturating_sub(1).min(5);
    Duration::from_secs(pseudo_jitter_secs(1u64 << exp)).min(RECONNECT_DELAY_MAX)
}

fn pseudo_jitter_secs(max_exclusive: u64) -> u64 {
    if max_exclusive <= 1 {
        return 0;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_nanos() as u64) % max_exclusive)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_attempt_zero_is_immediate() {
        assert_eq!(next_backoff(0), Duration::ZERO);
    }

    #[test]
    fn backoff_is_capped_at_thirty_seconds() {
        for attempt in 1..=20 {
            assert!(next_backoff(attempt) <= RECONNECT_DELAY_MAX);
        }
    }

    #[test]
    fn build_ws_uri_swaps_scheme() {
        let base = url::Url::parse("https://example.test").unwrap();
        let uri = build_ws_uri(&base).unwrap();
        assert!(uri.to_string().starts_with("wss://"));
        assert!(uri.path().ends_with("/api/ui-protocol/ws"));
    }

    #[test]
    fn build_request_sends_requested_capability_header() {
        let base = url::Url::parse("https://example.test").unwrap();
        let req = build_request(
            &base,
            &SecretString::new("tk"),
            &ProfileId::new("p1"),
            &Capabilities::requested(),
        )
        .unwrap();
        assert_eq!(
            req.headers()
                .get("x-octos-ui-features")
                .and_then(|v| v.to_str().ok()),
            Some("approval.typed.v1, pane.snapshots.v1, session.workspace_cwd.v1, context.lifecycle.v1")
        );
    }

    #[test]
    fn ephemeral_helper_only_message_delta() {
        assert!(is_ephemeral_method(methods::MESSAGE_DELTA));
        assert!(!is_ephemeral_method(methods::TOOL_STARTED));
    }
}
