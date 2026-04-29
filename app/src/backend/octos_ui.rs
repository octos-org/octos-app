//! `OctosUiAgent` — Makepad `Agent` implementation that bridges the UI to
//! `octos-app-transport`.
//!
//! Owns a Tokio runtime + the `(cmd_tx, evt_rx)` pair returned by
//! `octos_app_transport::ws::spawn`. UI calls translate to `OutboundCommand`s;
//! transport notifications drained on `handle_event` translate to `AgentEvent`s
//! the chat surface already understands.
//!
//! Crate boundary: `OctosUiAgent` is the *only* place inside `app/` that
//! talks to `octos-app-transport`. UI code goes through the `Agent` trait.

use std::collections::HashMap;

use makepad_ai::{Agent, AgentEvent, PromptId, SessionConfig, SessionId, StopReason};
use makepad_widgets::*;
use octos_app_store::state::{reduce as store_reduce, ConnectionEvent, Event as StoreEvent};
use octos_app_store::toasts::{Toast, ToastKind};
use octos_app_transport::{
    ws, Capabilities, ConnectionState, LifecycleResult, OutboundCommand, TransportConfig,
    TransportEvent,
};
use octos_core::ui_protocol::{
    InputItem, SessionOpenParams, TurnInterruptParams, TurnStartParams, UiCursor, UiNotification,
};
use octos_core::{ui_protocol::TurnId, SessionKey};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::app::sessions::APP_STATE;

/// `Agent` implementation backed by the Octos UI Protocol over WebSocket.
pub struct OctosUiAgent {
    /// Owned Tokio runtime — required because `ws::spawn` calls
    /// `tokio::spawn` internally and `app/` has no global runtime. Held for
    /// the agent's lifetime; the WS task lives inside it.
    _runtime: Runtime,
    /// Outbound side of the transport channel. Cloneable, lock-free
    /// `try_send` from the main thread.
    cmd_tx: Sender<OutboundCommand>,
    /// Inbound side; drained each tick on `handle_event`.
    evt_rx: Receiver<TransportEvent>,
    /// Makepad SessionId → octos-core SessionKey.
    session_keys: HashMap<SessionId, SessionKey>,
    /// octos-core SessionKey → Makepad SessionId (reverse lookup for
    /// notifications arriving from the wire).
    session_ids: HashMap<SessionKey, SessionId>,
    /// Sessions for which the server has answered `session/open`.
    ready_sessions: std::collections::HashSet<SessionId>,
    /// Makepad PromptId → octos-core TurnId.
    turn_ids: HashMap<PromptId, TurnId>,
    /// octos-core TurnId → Makepad PromptId (reverse lookup).
    prompt_ids: HashMap<TurnId, PromptId>,
    /// Most recent connection state — also mirrored into
    /// `APP_STATE.connection` (via `fold_connection_into_store`) for the
    /// top-bar status indicator and toast queue. Kept locally so we can
    /// detect transitions (Reconnecting → Live, Live → Failed, …) without
    /// re-reading the store under a write lock.
    connection_state: ConnectionState,
    /// Server-negotiated capability set. W05 reads this when deciding which
    /// approval / pane affordances to show. Stored as soon as
    /// `CapabilityNegotiated` arrives.
    #[allow(dead_code)]
    capabilities: Option<Capabilities>,
}

impl OctosUiAgent {
    /// Construct a new agent. Spawns the WebSocket task immediately so
    /// `create_session` can ship `session/open` on the first call.
    ///
    /// On a missing / invalid env (no bearer, unreachable URL) the transport
    /// task drops to `ConnectionState::Failed` after the budget expires; the
    /// agent stays usable, sends fail silently into a closed channel, and
    /// `is_session_ready` stays `false` forever — matching M1's "boots even
    /// without a server" requirement.
    pub fn new(config: TransportConfig) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("octos-ui-agent: tokio runtime build");
        let (cmd_tx, evt_rx) = {
            let _guard = runtime.enter();
            ws::spawn(config)
        };
        Self {
            _runtime: runtime,
            cmd_tx,
            evt_rx,
            session_keys: HashMap::new(),
            session_ids: HashMap::new(),
            ready_sessions: std::collections::HashSet::new(),
            turn_ids: HashMap::new(),
            prompt_ids: HashMap::new(),
            connection_state: ConnectionState::Idle,
            capabilities: None,
        }
    }

    /// Synthesise a fresh `SessionKey` from a Makepad `SessionId`. The
    /// LiveId-as-u64 → hex string round-trip is stable for the agent's
    /// lifetime; the server treats the value as an opaque identifier.
    fn make_session_key(session_id: SessionId) -> SessionKey {
        SessionKey(format!("octos-app:{:016x}", session_id.0.0))
    }

    /// Best-effort post to the transport task. Logs (and drops) on a closed
    /// or full channel — the caller can't usefully recover here.
    fn post(&self, cmd: OutboundCommand) {
        match self.cmd_tx.try_send(cmd) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                log::warn!("octos-ui-agent: command channel full; dropping");
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                log::warn!("octos-ui-agent: transport task gone; command dropped");
            }
        }
    }

    /// W05 — extract a cheap, send-safe handle for issuing
    /// `approval/respond` commands without cloning the whole agent. The
    /// handle wraps a `Sender<OutboundCommand>` plus a runtime handle, so
    /// `App::handle_actions` can fire `approval/respond` even though the
    /// agent itself is held behind `Box<dyn Agent>`.
    pub fn approval_handle(&self) -> ApprovalHandle {
        ApprovalHandle {
            cmd_tx: self.cmd_tx.clone(),
            runtime: self._runtime.handle().clone(),
        }
    }

    /// Translate one `TransportEvent` into zero-or-more `AgentEvent`s.
    /// Updates internal id maps and connection bookkeeping as a side effect.
    fn translate(&mut self, event: TransportEvent) -> Vec<AgentEvent> {
        match event {
            TransportEvent::ConnectionState(state) => {
                let prev = std::mem::replace(&mut self.connection_state, state.clone());
                self.fold_connection_into_store(&prev, &self.connection_state.clone());
                Vec::new()
            }
            TransportEvent::CapabilityNegotiated(caps) => {
                // W05 — mirror the negotiated capability flags into the
                // process-wide ApprovalsPane state. The widget reads
                // `APPROVAL_CAPS.read()` in `populate_card` to decide
                // whether to render typed sub-views and the scope dropdown.
                if let Ok(mut g) = crate::app::approvals::APPROVAL_CAPS.write() {
                    g.typed_approvals = caps.typed_approvals;
                    g.pane_snapshots = caps.pane_snapshots;
                }
                self.capabilities = Some(caps);
                Vec::new()
            }
            TransportEvent::RpcResult(LifecycleResult::SessionOpen(open)) => {
                let key = open.opened.session_id.clone();
                if let Some(sid) = self.session_ids.get(&key).copied() {
                    self.ready_sessions.insert(sid);
                    return vec![AgentEvent::SessionReady { session_id: sid }];
                }
                Vec::new()
            }
            TransportEvent::RpcResult(_) => Vec::new(),
            TransportEvent::RpcError {
                method, error, ..
            } => {
                let msg = format!("{method}: {} ({})", error.message, error.code);
                if method == "session/open" {
                    if let Some(&sid) = self.session_keys.keys().next() {
                        return vec![AgentEvent::SessionError {
                            session_id: sid,
                            error: msg,
                        }];
                    }
                }
                if let Some(&pid) = self.prompt_ids.values().next() {
                    return vec![AgentEvent::PromptError {
                        prompt_id: pid,
                        error: msg,
                    }];
                }
                log::warn!("octos-ui-agent: rpc error with no handler: {msg}");
                Vec::new()
            }
            TransportEvent::DurableNotification { payload, cursor } => {
                self.fold_into_store(payload.clone(), cursor);
                self.translate_notification(payload)
            }
            TransportEvent::EphemeralNotification { payload } => {
                // Ephemerals (`message/delta`) carry no cursor — pass `None`
                // so `state::reduce` skips the cursor advance per
                // `octos-app-store/src/state.rs:148-152`.
                self.fold_into_store(payload.clone(), None);
                self.translate_notification(payload)
            }
        }
    }

    /// Fold a `tool/*` / `task/*` / `turn/*` notification into the global
    /// `APP_STATE`. Replaces the W04-todo "buffer for now" behaviour the
    /// previous translate path used. The store's `apply_protocol`
    /// (`octos-app-store/src/state.rs:148`) already handles every
    /// `UiNotification` variant; we just hand off here.
    ///
    /// Read-write lock contention is bounded — the lock is held only for
    /// the duration of `reduce`, which is a small in-memory mutation. If a
    /// reader (the `TaskDock` widget, the `SessionList` widget) is mid-draw,
    /// we wait for it. This mirrors the `APP_STATE.write()` pattern used by
    /// `App::handle_actions` at `app/src/main.rs:2587-2599` when applying
    /// optimistic session deletes.
    fn fold_into_store(&self, n: UiNotification, cursor: Option<UiCursor>) {
        let event = StoreEvent::Protocol { cursor, notification: n };
        match APP_STATE.write() {
            Ok(mut state) => store_reduce(&mut state, event),
            Err(e) => log::warn!("octos-ui-agent: APP_STATE poisoned: {e}"),
        }
    }

    /// W04 follow-up #3: mirror connection-state transitions into the store
    /// so the top bar can render a coloured dot (Live = green, Reconnecting
    /// = amber, Failed/Idle = red) and push a transient toast on edges.
    /// Toasts are deduped by transition, not state, so a flap that lands on
    /// the same state still emits one toast (e.g. Live → Reconnecting →
    /// Live shows both reconnecting + reconnect-success). `prev == next`
    /// is a no-op: the transport may resend the current state on internal
    /// re-entrancy.
    fn fold_connection_into_store(&self, prev: &ConnectionState, next: &ConnectionState) {
        if prev == next {
            return;
        }
        let store_event = match next {
            ConnectionState::Live | ConnectionState::ReplayApplying => {
                Some(ConnectionEvent::Connected)
            }
            ConnectionState::Reconnecting { .. } => Some(ConnectionEvent::Reconnecting),
            ConnectionState::Idle
            | ConnectionState::Dialing
            | ConnectionState::Handshaking
            | ConnectionState::Failed => Some(ConnectionEvent::Offline),
        };
        let toast = match (prev, next) {
            // Reconnect arc: prev was a degraded state, we're back to Live.
            (
                ConnectionState::Reconnecting { .. } | ConnectionState::ReplayApplying,
                ConnectionState::Live,
            ) => Some(Toast::new(ToastKind::ReconnectSuccess, "Reconnected")),
            // Falling into Reconnecting from anywhere — show backoff toast.
            (_, ConnectionState::Reconnecting { attempt }) => Some(Toast::new(
                ToastKind::Reconnecting,
                format!("Reconnecting (attempt {attempt})"),
            )),
            // Cumulative budget exhausted — terminal failure toast.
            (_, ConnectionState::Failed) => Some(Toast::new(
                ToastKind::Error,
                "Connection failed; restart to retry",
            )),
            // First Live (Dialing/Handshaking → Live) — confirm online.
            (ConnectionState::Dialing | ConnectionState::Handshaking, ConnectionState::Live) => {
                Some(Toast::new(ToastKind::ReconnectSuccess, "Connected"))
            }
            _ => None,
        };
        match APP_STATE.write() {
            Ok(mut state) => {
                if let Some(ev) = store_event {
                    store_reduce(&mut state, StoreEvent::Connection(ev));
                }
                if let Some(t) = toast {
                    store_reduce(&mut state, StoreEvent::Toast(t));
                }
            }
            Err(e) => log::warn!("octos-ui-agent: APP_STATE poisoned: {e}"),
        }
    }

    fn translate_notification(&mut self, n: UiNotification) -> Vec<AgentEvent> {
        match n {
            UiNotification::MessageDelta(ev) => self
                .prompt_ids
                .get(&ev.turn_id)
                .copied()
                .map(|pid| {
                    vec![AgentEvent::TextDelta {
                        prompt_id: pid,
                        text: ev.text,
                    }]
                })
                .unwrap_or_default(),
            UiNotification::ToolStarted(ev) => self
                .prompt_ids
                .get(&ev.turn_id)
                .copied()
                .map(|pid| {
                    let input = ev
                        .arguments
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "{}".to_owned());
                    vec![AgentEvent::ToolRequest {
                        prompt_id: pid,
                        tool_use_id: ev.tool_call_id,
                        tool_name: ev.tool_name,
                        tool_input: input,
                    }]
                })
                .unwrap_or_default(),
            UiNotification::TurnCompleted(ev) => self
                .prompt_ids
                .remove(&ev.turn_id)
                .map(|pid| {
                    self.turn_ids.remove(&pid);
                    vec![AgentEvent::TurnComplete {
                        prompt_id: pid,
                        stop_reason: StopReason::EndTurn,
                    }]
                })
                .unwrap_or_default(),
            UiNotification::TurnError(ev) => self
                .prompt_ids
                .remove(&ev.turn_id)
                .map(|pid| {
                    self.turn_ids.remove(&pid);
                    vec![AgentEvent::PromptError {
                        prompt_id: pid,
                        error: format!("{}: {}", ev.code, ev.message),
                    }]
                })
                .unwrap_or_default(),
            // Drained into APP_STATE by `fold_into_store` above. The
            // TaskDock widget (`app/src/app/task_dock.rs`) reads them back
            // via `APP_STATE.tool_calls` / `APP_STATE.tasks` on each redraw,
            // so we don't bridge them through `AgentEvent` — there's no
            // round-trip required. ApprovalRequested lands on the
            // `ApprovalsSlice` (W05 surface). Warning toasts already land in
            // `state.toasts` via the store reducer.
            // Drained into APP_STATE via `fold_into_store`. Listed
            // explicitly so a new `UiNotification` variant tickles a compile
            // error here and forces a deliberate decision (forward-compat
            // per spec § 4.1).
            UiNotification::TurnStarted(_)
            | UiNotification::ToolProgress(_)
            | UiNotification::ToolCompleted(_)
            | UiNotification::TaskUpdated(_)
            | UiNotification::TaskOutputDelta(_)
            | UiNotification::ApprovalRequested(_)
            | UiNotification::ApprovalAutoResolved(_)
            | UiNotification::ApprovalDecided(_)
            | UiNotification::ApprovalCancelled(_)
            | UiNotification::ProgressUpdated(_)
            | UiNotification::ReplayLossy(_)
            | UiNotification::SessionOpened(_)
            | UiNotification::Warning(_) => Vec::new(),
        }
    }
}

impl Agent for OctosUiAgent {
    fn create_session(&mut self, _cx: &mut Cx, _config: SessionConfig) -> SessionId {
        let session_id = SessionId::new();
        let key = Self::make_session_key(session_id);
        self.session_keys.insert(session_id, key.clone());
        self.session_ids.insert(key.clone(), session_id);
        self.post(OutboundCommand::OpenSession(SessionOpenParams {
            session_id: key,
            profile_id: None,
            cwd: None,
            after: None,
        }));
        session_id
    }

    fn send_prompt(&mut self, _cx: &mut Cx, session_id: SessionId, text: &str) -> PromptId {
        let prompt_id = PromptId::new();
        let turn_id = TurnId::new();
        self.turn_ids.insert(prompt_id, turn_id.clone());
        self.prompt_ids.insert(turn_id.clone(), prompt_id);
        let Some(key) = self.session_keys.get(&session_id).cloned() else {
            log::warn!("octos-ui-agent: send_prompt for unknown session");
            return prompt_id;
        };
        self.post(OutboundCommand::StartTurn(TurnStartParams {
            session_id: key,
            turn_id,
            input: vec![InputItem::Text {
                text: text.to_owned(),
            }],
        }));
        prompt_id
    }

    fn send_tool_result(
        &mut self,
        _cx: &mut Cx,
        _session_id: SessionId,
        tool_use_id: &str,
        result: &str,
        is_error: bool,
    ) {
        // Best-effort placeholder — `tool/result` is not a stable wire method
        // yet (octos-app-transport flags this on `OutboundCommand::SendToolResult`).
        // W04+W05 will refine the shape once the server contract lands.
        let payload = serde_json::json!({
            "content": result,
            "is_error": is_error,
        });
        self.post(OutboundCommand::SendToolResult {
            tool_call_id: tool_use_id.to_owned(),
            result: payload,
        });
    }

    fn cancel_prompt(&mut self, _cx: &mut Cx, prompt_id: PromptId) {
        let Some(turn_id) = self.turn_ids.get(&prompt_id).cloned() else {
            return;
        };
        // M1 has at most one session per agent (W08 will multiplex). Pick
        // the first / only session_key for now; if there's no session, the
        // server will never see a cancel without a prior `turn/start`.
        let Some(key) = self.session_keys.values().next().cloned() else {
            return;
        };
        self.post(OutboundCommand::InterruptTurn(TurnInterruptParams {
            session_id: key,
            turn_id,
        }));
    }

    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        loop {
            match self.evt_rx.try_recv() {
                Ok(evt) => out.extend(self.translate(evt)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    log::warn!("octos-ui-agent: transport channel disconnected");
                    break;
                }
            }
        }
        out
    }

    fn is_session_ready(&self, session_id: SessionId) -> bool {
        self.ready_sessions.contains(&session_id)
    }

    fn is_stateless(&self) -> bool {
        // Octos sessions are stateful server-side.
        false
    }
}

/// W05 — handle exposed by `OctosUiAgent::approval_handle`. Carries a
/// cheap clone of the transport sender plus a runtime handle so the
/// approvals widget can post `approval/respond` and forward the wire
/// reply back to the UI thread without holding the agent itself. Cloning
/// is `Arc`-shaped under the hood (mpsc + tokio runtime handles).
#[derive(Clone)]
pub struct ApprovalHandle {
    cmd_tx: Sender<OutboundCommand>,
    runtime: tokio::runtime::Handle,
}

impl ApprovalHandle {
    /// Issue `approval/respond` and forward the wire reply to the UI
    /// thread as an `ApprovalAsyncAction`. Idempotent — the server
    /// enforces single-decision semantics; we just surface the outcome.
    /// See `workstreams/W05-approvals-diff.md` § "Approval response flow".
    pub fn respond(
        &self,
        session_id: SessionKey,
        approval_id: octos_core::ui_protocol::ApprovalId,
        decision: octos_core::ui_protocol::ApprovalDecision,
        scope: Option<String>,
    ) {
        use octos_core::ui_protocol::ApprovalRespondParams;
        use tokio::sync::oneshot;
        // `ApprovalDecision` is no longer `Copy` (FIX-01); clone it once for
        // the wire params and keep the original for the failure branch +
        // async reply.
        let mut params =
            ApprovalRespondParams::new(session_id, approval_id.clone(), decision.clone());
        params.approval_scope = scope;
        let (tx, rx) = oneshot::channel();
        let cmd = OutboundCommand::SendApprovalResponse { params, reply: tx };
        match self.cmd_tx.try_send(cmd) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
            | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Cx::post_action(crate::app::approvals::ApprovalAsyncAction {
                    approval_id,
                    decision,
                    outcome: crate::app::approvals::ApprovalAsyncOutcome::Failed {
                        message: "transport unavailable".to_owned(),
                        code: 0,
                        data: None,
                    },
                });
                return;
            }
        }
        let approval_id_for_task = approval_id.clone();
        self.runtime.spawn(async move {
            let outcome = match rx.await {
                Ok(Ok(res)) => crate::app::approvals::ApprovalAsyncOutcome::Accepted {
                    runtime_resumed: res.runtime_resumed,
                },
                // Forward the structured RpcError so the UI can detect
                // `-32011 APPROVAL_NOT_PENDING` and recover the decision
                // from `data.recorded_decision`. See
                // `octos-cli/src/api/ui_protocol_approvals.rs:198-215`.
                Ok(Err(err)) => crate::app::approvals::ApprovalAsyncOutcome::Failed {
                    message: err.message.clone(),
                    code: err.code,
                    data: err.data.clone(),
                },
                Err(_) => crate::app::approvals::ApprovalAsyncOutcome::Failed {
                    message: "transport dropped the reply channel".to_owned(),
                    code: 0,
                    data: None,
                },
            };
            Cx::post_action(crate::app::approvals::ApprovalAsyncAction {
                approval_id: approval_id_for_task,
                decision,
                outcome,
            });
        });
    }
}
