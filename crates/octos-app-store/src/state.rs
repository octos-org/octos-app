//! Top-level `AppState` and reducer entry point.
//!
//! See `01-ARCHITECTURE.md` § 5 (state model) and § 7 (failure model). Pure:
//! no I/O. Side effects (REST, WS dispatch, keychain) are decided by the
//! caller from the resulting state, not initiated here.

use crate::approvals::ApprovalsSlice;
use crate::auth::{AuthEvent, AuthSlice};
use crate::files::{FileHandle, FileMeta};
use crate::navigation::{self, CurrentScreen, NavigationEvent};
use crate::sessions::{Session, SessionMap};
use crate::tasks::{Task, ToolCall, ToolCallId};
use crate::toasts::{Toast, ToastKind, ToastQueue};
use crate::turns::Turn;
use chrono::Utc;
use octos_core::app_ui::AppUiBackendEvent as UiNotification;
// see octos-core ui_protocol.rs:62 (UiCursor), :69 (TurnId)
use octos_core::ui_protocol::{TaskRuntimeState, TurnId, UiCursor};
use octos_core::{SessionKey, TaskId};
use std::collections::HashMap;

/// Connection state machine. Mirrors `01-ARCHITECTURE.md` § 7. Redefined
/// here (not re-exported) to avoid a `store → transport → store` dep cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Reconnecting,
    #[default]
    Offline,
}

/// State that does NOT survive reconnect. `message/delta` is non-durable;
/// see `03-PROTOCOL-CONTRACT.md` § Live streaming output.
#[derive(Clone, Debug, Default)]
pub struct Ephemeral {
    pub streaming_text: HashMap<TurnId, String>,
    pub thinking_text: HashMap<TurnId, String>,
}

#[derive(Clone, Debug, Default)]
pub struct AppState {
    pub auth: AuthSlice,
    pub navigation: CurrentScreen,
    pub sessions: SessionMap,
    pub current_session: Option<SessionKey>,
    /// Last applied `UiCursor` per session. Persisted to SQLite by the
    /// binary; see `01-ARCHITECTURE.md` § 6.
    pub cursor: HashMap<SessionKey, UiCursor>,
    pub turns: HashMap<TurnId, Turn>,
    pub tasks: HashMap<TaskId, Task>,
    pub tool_calls: HashMap<ToolCallId, ToolCall>,
    pub files: HashMap<FileHandle, FileMeta>,
    pub approvals: ApprovalsSlice,
    pub ephemeral: Ephemeral,
    pub toasts: ToastQueue,
    pub connection: ConnectionState,
}

impl AppState {
    pub fn new() -> Self { Self::default() }
}

/// REST snapshot wrappers. Cold-open hydrate fans through the same reducer.
#[derive(Clone, Debug)]
pub enum SnapshotEvent {
    SessionsHydrated(Vec<Session>),
    SessionRemoved(SessionKey),
    FileMetaHydrated(FileMeta),
}

/// Connection events from `octos-app-transport`. Local-only; defined here
/// to break the dep cycle.
#[derive(Clone, Copy, Debug)]
pub enum ConnectionEvent { Connected, Reconnecting, Offline }

/// Top-level event dispatched at the reducer.
#[derive(Clone, Debug)]
pub enum Event {
    /// Server notification + optional cursor (None for ephemeral kinds).
    Protocol { cursor: Option<UiCursor>, notification: UiNotification },
    Auth(AuthEvent),
    Navigation(NavigationEvent),
    Snapshot(SnapshotEvent),
    Connection(ConnectionEvent),
    Toast(Toast),
    DismissOldestToast,
}

/// Apply an `Event`. Pure transition over `&mut AppState`.
pub fn reduce(state: &mut AppState, event: Event) {
    match event {
        Event::Auth(ev) => crate::auth::reduce(&mut state.auth, ev),
        Event::Navigation(ev) => {
            let logging_out = matches!(ev, NavigationEvent::Logout);
            navigation::reduce(&mut state.navigation, &mut state.current_session, ev);
            if logging_out {
                // Treat Login as a fresh slate: drop ephemeral + transient toasts.
                state.ephemeral = Ephemeral::default();
                state.toasts = ToastQueue::default();
            }
        }
        Event::Snapshot(ev) => apply_snapshot(state, ev),
        Event::Connection(ev) => state.connection = match ev {
            ConnectionEvent::Connected => ConnectionState::Connected,
            ConnectionEvent::Reconnecting => ConnectionState::Reconnecting,
            ConnectionEvent::Offline => ConnectionState::Offline,
        },
        Event::Toast(t) => state.toasts.push(t),
        Event::DismissOldestToast => { state.toasts.dismiss_oldest(); }
        Event::Protocol { cursor, notification } => apply_protocol(state, cursor, notification),
    }
}

fn apply_snapshot(state: &mut AppState, ev: SnapshotEvent) {
    match ev {
        SnapshotEvent::SessionsHydrated(list) => for s in list { state.sessions.insert(s); },
        SnapshotEvent::SessionRemoved(id) => {
            state.sessions.remove(&id);
            if state.current_session.as_ref() == Some(&id) {
                state.current_session = None;
                state.navigation = CurrentScreen::Home;
            }
            state.cursor.remove(&id);
        }
        SnapshotEvent::FileMetaHydrated(meta) => {
            state.files.insert(meta.handle.clone(), meta);
        }
    }
}

/// `session_id` for any notification — used to advance cursor + flags.
///
/// Returns `None` for forward-compat / future-unknown variants so the caller
/// can skip the cursor advance step gracefully (per `03-PROTOCOL-CONTRACT.md`
/// § 4.1 capability negotiation — clients MUST tolerate unknown variants).
fn route_session(n: &UiNotification) -> Option<&SessionKey> {
    match n {
        UiNotification::SessionOpened(e) => Some(&e.session_id),
        UiNotification::TurnStarted(e) => Some(&e.session_id),
        UiNotification::MessageDelta(e) => Some(&e.session_id),
        UiNotification::ToolStarted(e) => Some(&e.session_id),
        UiNotification::ToolProgress(e) => Some(&e.session_id),
        UiNotification::ToolCompleted(e) => Some(&e.session_id),
        UiNotification::ApprovalRequested(e) => Some(&e.session_id),
        UiNotification::ApprovalAutoResolved(e) => Some(&e.session_id),
        UiNotification::ApprovalDecided(e) => Some(&e.session_id),
        UiNotification::ApprovalCancelled(e) => Some(&e.session_id),
        UiNotification::TaskUpdated(e) => Some(&e.session_id),
        UiNotification::TaskOutputDelta(e) => Some(&e.session_id),
        UiNotification::ProgressUpdated(e) => Some(&e.session_id),
        UiNotification::ReplayLossy(e) => Some(&e.session_id),
        UiNotification::Warning(e) => Some(&e.session_id),
        UiNotification::TurnCompleted(e) => Some(&e.session_id),
        UiNotification::TurnError(e) => Some(&e.session_id),
    }
}

fn apply_protocol(state: &mut AppState, cursor: Option<UiCursor>, n: UiNotification) {
    let now = Utc::now();
    // Cursor advance — `MessageDelta` carries `cursor: None` from the transport.
    // `route_session` returns `None` for unknown future variants, in which case
    // we skip the advance (forward-compat per spec § 4.1).
    if let Some(c) = cursor {
        if let Some(sid) = route_session(&n) {
            state.cursor.insert(sid.clone(), c);
        }
    }
    match n {
        UiNotification::SessionOpened(e) => {
            mark_streaming(state, &e.session_id, false);
        }
        UiNotification::TurnStarted(e) => {
            let entry = state.turns.entry(e.turn_id.clone()).or_insert_with(|| {
                Turn::started(e.turn_id.clone(), e.session_id.clone(), now)
            });
            entry.started_at = e.timestamp;
            mark_streaming(state, &e.session_id, true);
        }
        UiNotification::MessageDelta(e) => {
            state.ephemeral.streaming_text.entry(e.turn_id.clone())
                .or_default().push_str(&e.text);
            if let Some(t) = state.turns.get_mut(&e.turn_id) { t.mark_streaming(); }
            mark_streaming(state, &e.session_id, true);
        }
        UiNotification::ToolStarted(e) => {
            let id = ToolCallId::from(e.tool_call_id);
            state.tool_calls.insert(id.clone(), ToolCall::started(
                id, e.session_id.clone(), e.turn_id, e.tool_name, e.arguments, now,
            ));
            mark_active_task(state, &e.session_id, true);
        }
        UiNotification::ToolProgress(e) => {
            // Upsert a stub if `started` was missed (defensive — protocol
            // should never reorder, but a missed-event is cheap to recover).
            let id = ToolCallId::from(e.tool_call_id);
            let entry = state.tool_calls.entry(id.clone()).or_insert_with(|| {
                ToolCall::started(
                    id.clone(),
                    e.session_id.clone(),
                    e.turn_id,
                    "<unknown>",
                    None,
                    now,
                )
            });
            // W04 follow-up #4 — store the latest fraction so TaskDock can
            // aggregate average progress across concurrent tools.
            if let Some(p) = e.progress_pct {
                entry.progress_pct = Some(p);
            }
        }
        UiNotification::ToolCompleted(e) => {
            let id = ToolCallId::from(e.tool_call_id);
            if let Some(tc) = state.tool_calls.get_mut(&id) {
                tc.mark_completed(e.success, e.output_preview, now);
            }
            recompute_active(state, &e.session_id);
        }
        UiNotification::ApprovalRequested(e) => state.approvals.requested(e),
        UiNotification::TaskUpdated(e) => {
            let task = state.tasks.entry(e.task_id.clone()).or_insert_with(|| {
                Task::new(e.task_id.clone(), e.session_id.clone(), now)
            });
            // Lower-cased typed state for the dock; unknown future variants
            // pass through via Debug.
            task.runtime_state = format!("{:?}", e.state).to_lowercase();
            task.summary = Some(e.title.clone());
            if let Some(detail) = e.runtime_detail { task.lifecycle_state = detail; }
            task.last_updated = now;
            let active = !matches!(e.state, TaskRuntimeState::Completed | TaskRuntimeState::Failed);
            if active {
                mark_active_task(state, &e.session_id, true);
            } else {
                recompute_active(state, &e.session_id);
            }
        }
        UiNotification::TaskOutputDelta(e) => {
            if let Some(t) = state.tasks.get_mut(&e.task_id) {
                t.last_cursor = Some(e.cursor);
                t.last_updated = now;
            }
        }
        UiNotification::Warning(e) => state.toasts.push(
            Toast::new(ToastKind::Info, format!("{}: {}", e.code, e.message)),
        ),
        UiNotification::TurnCompleted(e) => {
            if let Some(t) = state.turns.get_mut(&e.turn_id) { t.mark_completed(now); }
            // Drop the ephemeral buffer; durable history rehydrates via REST.
            state.ephemeral.streaming_text.remove(&e.turn_id);
            state.ephemeral.thinking_text.remove(&e.turn_id);
            // The completion event's own cursor is the canonical resume point.
            if let Some(c) = e.cursor { state.cursor.insert(e.session_id.clone(), c); }
            mark_streaming(state, &e.session_id, false);
            recompute_active(state, &e.session_id);
        }
        UiNotification::TurnError(e) => {
            if let Some(t) = state.turns.get_mut(&e.turn_id) {
                t.mark_error(&e.code, e.message.clone(), now);
            }
            state.ephemeral.streaming_text.remove(&e.turn_id);
            state.ephemeral.thinking_text.remove(&e.turn_id);
            mark_streaming(state, &e.session_id, false);
            recompute_active(state, &e.session_id);
            state.toasts.push(Toast::new(ToastKind::Error, e.message));
        }
        UiNotification::ProgressUpdated(e) => {
            // `progress/updated` is the rich-progress channel — at this layer
            // we only care about `progress_pct`, which we forward to the
            // matching `ToolCall` (when the metadata's correlation lets us
            // identify one) and to the `Task.summary` if present. The
            // TaskDock widget reads both fields on each redraw.
            //
            // We don't have a tool_call_id on this payload, so we update by
            // turn_id when known: any in-flight tool call on this turn whose
            // own progress field is older gets the latest fraction. This is a
            // best-effort enrichment; the canonical per-tool fraction still
            // arrives via `tool/progress`.
            if let Some(pct) = e.metadata.progress_pct {
                if let Some(tid) = e.turn_id.as_ref() {
                    for tc in state.tool_calls.values_mut() {
                        if &tc.turn_id == tid && tc.completed_at.is_none() {
                            tc.progress_pct = Some(pct);
                        }
                    }
                }
            }
            // Surface a human-readable update on any task tied to this
            // session — `label` / `message` from the metadata are the most
            // user-facing fields.
            if let Some(label) = e.metadata.label.clone().or_else(|| e.metadata.message.clone()) {
                for task in state.tasks.values_mut() {
                    if task.session_id == e.session_id
                        && !matches!(task.runtime_state.as_str(), "completed" | "failed")
                    {
                        task.summary = Some(label.clone());
                        task.last_updated = now;
                    }
                }
            }
        }
        UiNotification::ApprovalAutoResolved(e) => {
            // The server short-circuited the approval via a recorded scope
            // rule. If the same approval was already surfaced as a card
            // (race-y but possible — e.g. a turn that flipped to auto-resolve
            // mid-flight), mark it Decided. Otherwise just toast: the user
            // never saw a card to update. `decided()` is a no-op when
            // `by_id` doesn't contain the id, which is exactly the
            // non-surfaced case.
            state.approvals.decided(&e.approval_id, e.decision.clone());
            let label = match &e.decision {
                octos_core::ui_protocol::ApprovalDecision::Approve => "Auto-approved",
                octos_core::ui_protocol::ApprovalDecision::Deny => "Auto-denied",
                octos_core::ui_protocol::ApprovalDecision::Unknown(s) => {
                    // Forward-compat: unrecognised decision string — show
                    // raw value so the user can see why.
                    state.toasts.push(Toast::new(
                        ToastKind::Info,
                        format!("Auto-resolved ({s}) by {} scope", e.scope),
                    ));
                    return;
                }
            };
            state.toasts.push(Toast::new(
                ToastKind::Info,
                format!("{label} ({} scope)", e.scope),
            ));
        }
        UiNotification::ApprovalDecided(e) => {
            let known = state.approvals.by_id.contains_key(&e.approval_id);
            state.approvals.decided(&e.approval_id, e.decision.clone());
            if known {
                state.toasts.push(Toast::new(
                    ToastKind::Info,
                    format!(
                        "Approval decided by {}: {}",
                        e.decided_by,
                        e.decision.as_wire_str()
                    ),
                ));
            }
        }
        UiNotification::ApprovalCancelled(e) => {
            let known = state.approvals.by_id.contains_key(&e.approval_id);
            state.approvals.cancelled(&e.approval_id, e.reason.clone());
            if known {
                state.toasts.push(Toast::new(
                    ToastKind::Info,
                    format!("Approval cancelled: {}", e.reason),
                ));
            }
        }
        UiNotification::ReplayLossy(e) => {
            if let Some(cursor) = e.last_durable_cursor {
                state.cursor.insert(e.session_id.clone(), cursor);
            }
            state.toasts.push(Toast::new(
                ToastKind::Reconnecting,
                format!(
                    "Replay dropped {} notifications; rehydrating session",
                    e.dropped_count
                ),
            ));
        }
    }
}

fn mark_streaming(state: &mut AppState, sid: &SessionKey, on: bool) {
    if let Some(s) = state.sessions.get_mut(sid) { s.is_streaming = on; }
}

fn mark_active_task(state: &mut AppState, sid: &SessionKey, on: bool) {
    if let Some(s) = state.sessions.get_mut(sid) { s.has_active_task = on; }
}

/// Recompute `has_active_task` by scanning tool/task maps. Cheap (n is small).
fn recompute_active(state: &mut AppState, sid: &SessionKey) {
    let open_tool = state.tool_calls.values()
        .any(|tc| &tc.session_id == sid && tc.completed_at.is_none());
    let open_task = state.tasks.values().any(|t| &t.session_id == sid
        && !matches!(t.runtime_state.as_str(), "completed" | "failed"));
    mark_active_task(state, sid, open_tool || open_task);
}

/// Type alias the binary can store to disk without re-importing octos-core.
pub type UiCursorMap = HashMap<SessionKey, UiCursor>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ProfileId;
    use crate::sessions::Session;
    use crate::toasts::ToastKind;
    use crate::turns::TurnStatus;
    use chrono::DateTime;
    use octos_core::ui_protocol::{
        ApprovalCancelledEvent, ApprovalDecidedEvent, ApprovalDecision, ApprovalId,
        MessageDeltaEvent, ReplayLossyEvent, TaskRuntimeState, TaskUpdatedEvent,
        ToolCompletedEvent, ToolStartedEvent, TurnCompletedEvent, TurnStartedEvent,
    };
    use uuid::Uuid;

    fn ts(secs: i64) -> DateTime<Utc> { DateTime::<Utc>::from_timestamp(secs, 0).unwrap() }
    fn key(s: &str) -> SessionKey { SessionKey(s.into()) }
    fn pid() -> ProfileId { ProfileId::from("acme".to_owned()) }
    fn turn(n: u128) -> TurnId { TurnId(Uuid::from_u128(n)) }

    fn seed_session(state: &mut AppState, k: &str) {
        state.sessions.insert(Session::new(key(k), pid(), "S", ts(0)));
    }

    #[test]
    fn reduce_durable_notification_updates_cursor() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let cursor = UiCursor { stream: "main".into(), seq: 7 };
        let ev = Event::Protocol {
            cursor: Some(cursor.clone()),
            notification: UiNotification::TurnStarted(TurnStartedEvent {
                session_id: key("t:1"),
                turn_id: turn(1),
                timestamp: ts(100),
            }),
        };
        reduce(&mut s, ev);
        assert_eq!(s.cursor.get(&key("t:1")), Some(&cursor));
        assert!(s.turns.contains_key(&turn(1)));
        assert!(s.sessions.get(&key("t:1")).unwrap().is_streaming);
    }

    #[test]
    fn message_delta_buffers_ephemeral_text_and_does_not_advance_cursor() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        s.turns.insert(turn(1), Turn::started(turn(1), key("t:1"), ts(0)));
        for chunk in ["hel", "lo"] {
            reduce(&mut s, Event::Protocol {
                cursor: None,
                notification: UiNotification::MessageDelta(MessageDeltaEvent {
                    session_id: key("t:1"), turn_id: turn(1), text: chunk.into(),
                }),
            });
        }
        assert_eq!(s.ephemeral.streaming_text.get(&turn(1)).unwrap(), "hello");
        assert!(s.cursor.get(&key("t:1")).is_none());
        assert_eq!(s.turns.get(&turn(1)).unwrap().status, TurnStatus::Streaming);
    }

    #[test]
    fn turn_completed_drops_ephemeral_buffer_and_clears_streaming_dot() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        s.turns.insert(turn(1), Turn::started(turn(1), key("t:1"), ts(0)));
        s.ephemeral.streaming_text.insert(turn(1), "partial".into());
        s.sessions.get_mut(&key("t:1")).unwrap().is_streaming = true;
        let cursor = UiCursor { stream: "main".into(), seq: 42 };
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::TurnCompleted(TurnCompletedEvent {
                session_id: key("t:1"), turn_id: turn(1), cursor: Some(cursor.clone()),
            }),
        });
        assert!(!s.ephemeral.streaming_text.contains_key(&turn(1)));
        assert_eq!(s.turns.get(&turn(1)).unwrap().status, TurnStatus::Completed);
        // TurnCompleted's own cursor still wins even when the outer is None.
        assert_eq!(s.cursor.get(&key("t:1")), Some(&cursor));
        assert!(!s.sessions.get(&key("t:1")).unwrap().is_streaming);
    }

    #[test]
    fn tool_lifecycle_lights_and_clears_active_task_dot() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ToolStarted(ToolStartedEvent {
                session_id: key("t:1"), turn_id: turn(1),
                tool_call_id: "call-x".into(), tool_name: "shell".into(), arguments: None,
            }),
        });
        assert!(s.sessions.get(&key("t:1")).unwrap().has_active_task);
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ToolCompleted(ToolCompletedEvent {
                session_id: key("t:1"), turn_id: turn(1),
                tool_call_id: "call-x".into(), tool_name: "shell".into(),
                success: Some(true), output_preview: Some("done".into()), duration_ms: Some(10),
            }),
        });
        assert!(!s.sessions.get(&key("t:1")).unwrap().has_active_task);
        let tc = s.tool_calls.get(&ToolCallId::from("call-x")).unwrap();
        assert_eq!(tc.success, Some(true));
        assert_eq!(tc.output_preview.as_deref(), Some("done"));
    }

    #[test]
    fn task_updated_running_then_completed() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let tid = TaskId::default();
        for st in [TaskRuntimeState::Running, TaskRuntimeState::Completed] {
            reduce(&mut s, Event::Protocol {
                cursor: None,
                notification: UiNotification::TaskUpdated(TaskUpdatedEvent {
                    session_id: key("t:1"), task_id: tid.clone(),
                    title: "build".into(), state: st, runtime_detail: None,
                }),
            });
        }
        assert!(!s.sessions.get(&key("t:1")).unwrap().has_active_task);
    }

    #[test]
    fn approval_requested_lands_in_slice() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let aid = ApprovalId(Uuid::from_u128(7));
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalRequested(
                octos_core::ui_protocol::ApprovalRequestedEvent::generic(
                    key("t:1"), aid.clone(), turn(1), "shell", "Run", "ls",
                ),
            ),
        });
        assert_eq!(s.approvals.pending_count(), 1);
        s.approvals.pending_response(&aid, ApprovalDecision::Approve);
        s.approvals.decided(&aid, ApprovalDecision::Approve);
        assert_eq!(s.approvals.pending_count(), 0);
    }

    #[test]
    fn approval_decided_notification_collapses_pending_card() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let aid = ApprovalId(Uuid::from_u128(7));
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalRequested(
                octos_core::ui_protocol::ApprovalRequestedEvent::generic(
                    key("t:1"), aid.clone(), turn(1), "shell", "Run", "ls",
                ),
            ),
        });

        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalDecided(ApprovalDecidedEvent::manual(
                key("t:1"),
                aid.clone(),
                turn(1),
                ApprovalDecision::Approve,
                "server",
            )),
        });

        assert_eq!(s.approvals.pending_count(), 0);
        assert!(s.toasts.iter().any(|t| {
            t.kind == ToastKind::Info && t.message == "Approval decided by server: approve"
        }));
    }

    #[test]
    fn approval_cancelled_notification_collapses_pending_card() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let aid = ApprovalId(Uuid::from_u128(7));
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalRequested(
                octos_core::ui_protocol::ApprovalRequestedEvent::generic(
                    key("t:1"), aid.clone(), turn(1), "shell", "Run", "ls",
                ),
            ),
        });

        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalCancelled(
                ApprovalCancelledEvent::turn_interrupted(key("t:1"), aid.clone(), turn(1)),
            ),
        });

        assert_eq!(s.approvals.pending_count(), 0);
        assert!(s.toasts.iter().any(|t| {
            t.kind == ToastKind::Info && t.message == "Approval cancelled: turn_interrupted"
        }));
    }

    #[test]
    fn replay_lossy_notification_records_last_durable_cursor() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let cursor = UiCursor { stream: "main".into(), seq: 44 };

        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ReplayLossy(ReplayLossyEvent {
                session_id: key("t:1"),
                dropped_count: 2,
                last_durable_cursor: Some(cursor.clone()),
            }),
        });

        assert_eq!(s.cursor.get(&key("t:1")), Some(&cursor));
        assert!(s.toasts.iter().any(|t| {
            t.kind == ToastKind::Reconnecting
                && t.message == "Replay dropped 2 notifications; rehydrating session"
        }));
    }

    #[test]
    fn logout_clears_navigation_and_ephemeral() {
        let mut s = AppState::new();
        s.current_session = Some(key("t:1"));
        s.navigation = CurrentScreen::Chat { session: Some(key("t:1")) };
        s.ephemeral.streaming_text.insert(turn(1), "x".into());
        s.toasts.push(Toast::new(ToastKind::Info, "hello"));
        reduce(&mut s, Event::Navigation(NavigationEvent::Logout));
        assert_eq!(s.navigation, CurrentScreen::Login);
        assert_eq!(s.current_session, None);
        assert!(s.ephemeral.streaming_text.is_empty());
        assert!(s.toasts.is_empty());
    }

    #[test]
    fn snapshot_session_removed_clears_pointer_and_cursor() {
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        s.current_session = Some(key("t:1"));
        s.navigation = CurrentScreen::Chat { session: Some(key("t:1")) };
        s.cursor.insert(key("t:1"), UiCursor { stream: "m".into(), seq: 1 });
        reduce(&mut s, Event::Snapshot(SnapshotEvent::SessionRemoved(key("t:1"))));
        assert!(s.sessions.get(&key("t:1")).is_none());
        assert_eq!(s.current_session, None);
        assert_eq!(s.navigation, CurrentScreen::Home);
        assert!(s.cursor.get(&key("t:1")).is_none());
    }

    #[test]
    fn connection_event_round_trips() {
        let mut s = AppState::new();
        reduce(&mut s, Event::Connection(ConnectionEvent::Reconnecting));
        assert_eq!(s.connection, ConnectionState::Reconnecting);
        reduce(&mut s, Event::Connection(ConnectionEvent::Connected));
        assert_eq!(s.connection, ConnectionState::Connected);
    }

    /// FIX-01 follow-up — `progress/updated` enriches in-flight tool calls
    /// with the latest `progress_pct` and updates active task summaries.
    #[test]
    fn progress_updated_forwards_pct_and_summary() {
        use octos_core::ui_protocol::{
            ProgressUpdatedEvent, UiProgressMetadata,
        };
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        // Seed an in-flight tool call on turn 1 so the enrichment lands.
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ToolStarted(ToolStartedEvent {
                session_id: key("t:1"),
                turn_id: turn(1),
                tool_call_id: "call-x".into(),
                tool_name: "shell".into(),
                arguments: None,
            }),
        });
        // And a running task on the same session.
        let tid = TaskId::default();
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::TaskUpdated(TaskUpdatedEvent {
                session_id: key("t:1"),
                task_id: tid.clone(),
                title: "build".into(),
                state: TaskRuntimeState::Running,
                runtime_detail: None,
            }),
        });
        let mut metadata = UiProgressMetadata::new("status");
        metadata.progress_pct = Some(0.42);
        metadata.label = Some("compiling …".into());
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ProgressUpdated(ProgressUpdatedEvent::new(
                key("t:1"),
                Some(turn(1)),
                metadata,
            )),
        });
        let tc = s.tool_calls.get(&ToolCallId::from("call-x")).unwrap();
        assert_eq!(tc.progress_pct, Some(0.42));
        let task = s.tasks.get(&tid).unwrap();
        assert_eq!(task.summary.as_deref(), Some("compiling …"));
    }

    /// FIX-06 — `approval/auto_resolved` toasts the user and marks any
    /// pre-existing card as Decided so the UI doesn't keep showing buttons.
    #[test]
    fn approval_auto_resolved_toasts_and_decides() {
        use octos_core::ui_protocol::{
            ApprovalAutoResolvedEvent, ApprovalRequestedEvent,
        };
        let mut s = AppState::new();
        seed_session(&mut s, "t:1");
        let aid = ApprovalId(Uuid::from_u128(7));
        // Pre-existing pending card (e.g. surfaced before the auto-resolve
        // race resolved server-side).
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalRequested(
                ApprovalRequestedEvent::generic(key("t:1"), aid.clone(), turn(1), "shell", "Run", "ls"),
            ),
        });
        assert_eq!(s.approvals.pending_count(), 1);
        reduce(&mut s, Event::Protocol {
            cursor: None,
            notification: UiNotification::ApprovalAutoResolved(ApprovalAutoResolvedEvent {
                session_id: key("t:1"),
                approval_id: aid.clone(),
                turn_id: turn(1),
                tool_name: "shell".into(),
                scope: "session".into(),
                scope_match: "exact".into(),
                decision: ApprovalDecision::Approve,
            }),
        });
        // Card collapsed.
        assert_eq!(s.approvals.pending_count(), 0);
        // Toast surfaced.
        assert!(s.toasts.iter().any(|t| t.kind == ToastKind::Info
            && t.message.contains("Auto-approved")));
    }
}
