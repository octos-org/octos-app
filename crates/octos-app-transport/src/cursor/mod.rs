//! Per-session cursor tracking.
//!
//! `UiCursor` is the resumable consumption position for the per-session
//! ledger (see octos-core ui_protocol.rs:62). Transport keeps the last applied
//! cursor in memory; the `CursorPersist` callback lets W04 plug in SQLite
//! without changing this surface.

use std::collections::HashMap;

use octos_core::{SessionKey, ui_protocol::UiCursor};

/// In-memory `SessionKey → UiCursor`. Newtype so the underlying container
/// can change later (e.g. write-through wrapper) without touching callers.
#[derive(Debug, Default, Clone)]
pub struct CursorStore {
    inner: HashMap<SessionKey, UiCursor>,
}

impl CursorStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get(&self, session: &SessionKey) -> Option<&UiCursor> {
        self.inner.get(session)
    }
    pub fn set(&mut self, session: SessionKey, cursor: UiCursor) {
        self.inner.insert(session, cursor);
    }
    pub fn drop(&mut self, session: &SessionKey) {
        self.inner.remove(session);
    }
    /// Synonym for `drop` — kept because `drop` shadows the prelude trait
    /// name in some grep contexts. Prefer this in new code.
    pub fn delete(&mut self, session: &SessionKey) {
        self.inner.remove(session);
    }
    pub fn iter(&self) -> impl Iterator<Item = (&SessionKey, &UiCursor)> {
        self.inner.iter()
    }
}

/// Pluggable persistence for cursors. W04 implements this against SQLite.
/// Errors flatten to `String` to stay free of W04's error type. Transport
/// logs and continues on persistence failure — losing a cursor downgrades to
/// a REST rehydrate, never to data corruption.
pub trait CursorPersist: Send + Sync + 'static {
    fn load(&self, session: &SessionKey) -> Result<Option<UiCursor>, String>;
    fn save(&self, session: &SessionKey, cursor: &UiCursor) -> Result<(), String>;
    fn forget(&self, session: &SessionKey) -> Result<(), String>;
}

/// No-op impl for tests and the in-memory-only path.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCursorPersist;

impl CursorPersist for NoopCursorPersist {
    fn load(&self, _: &SessionKey) -> Result<Option<UiCursor>, String> { Ok(None) }
    fn save(&self, _: &SessionKey, _: &UiCursor) -> Result<(), String> { Ok(()) }
    fn forget(&self, _: &SessionKey) -> Result<(), String> { Ok(()) }
}

// TODO(W04): wire a SQLite-backed `CursorPersist` here so cursors survive
// process restarts. Until then, transport runs with `NoopCursorPersist` and
// re-hydrates from REST on reconnect when the in-memory cursor is lost.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_set_get_delete_round_trip() {
        let key = SessionKey::new("cli", "demo");
        let cur = UiCursor { stream: "main".into(), seq: 7 };
        let mut s = CursorStore::new();
        assert!(s.get(&key).is_none());
        s.set(key.clone(), cur.clone());
        assert_eq!(s.get(&key), Some(&cur));
        s.delete(&key);
        assert!(s.get(&key).is_none());
    }
}
