//! Capability negotiation.
//!
//! Server advertises supported features via `UiProtocolCapabilities`
//! (octos-core ui_protocol.rs:369). Two known v1 flags get typed booleans;
//! unknown future flags stay in `raw` per the contract's forward-compat rule.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// see octos-core ui_protocol.rs:29
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_APPROVAL_TYPED_V1 as APPROVAL_TYPED_V1;
// see octos-core ui_protocol.rs:32
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_PANE_SNAPSHOTS_V1 as PANE_SNAPSHOTS_V1;
// see octos-core ui_protocol.rs:33
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_SESSION_WORKSPACE_CWD_V1 as SESSION_WORKSPACE_CWD_V1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub typed_approvals: bool,
    pub pane_snapshots: bool,
    pub session_workspace_cwd: bool,
    /// Everything the server advertised, including unknown features.
    #[serde(default)]
    pub raw: BTreeMap<String, Value>,
}

impl Capabilities {
    /// Default desired capabilities — request both known flags.
    pub fn requested() -> Self {
        Self {
            typed_approvals: true,
            pane_snapshots: true,
            session_workspace_cwd: true,
            raw: BTreeMap::new(),
        }
    }

    /// Deterministic header value for `X-Octos-Ui-Features`.
    pub fn feature_header_value(&self) -> String {
        let mut features = Vec::new();
        if self.typed_approvals {
            features.push(APPROVAL_TYPED_V1.to_owned());
        }
        if self.pane_snapshots {
            features.push(PANE_SNAPSHOTS_V1.to_owned());
        }
        if self.session_workspace_cwd {
            features.push(SESSION_WORKSPACE_CWD_V1.to_owned());
        }
        for (name, value) in &self.raw {
            if value.as_bool().unwrap_or(false)
                && name != APPROVAL_TYPED_V1
                && name != PANE_SNAPSHOTS_V1
                && name != SESSION_WORKSPACE_CWD_V1
            {
                features.push(name.clone());
            }
        }
        features.join(", ")
    }

    /// Build from the server's `supported_features` list (see octos-core
    /// ui_protocol.rs:374).
    pub fn from_supported_features<I, S>(features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut caps = Self::default();
        for feature in features {
            let name = feature.as_ref();
            match name {
                APPROVAL_TYPED_V1 => caps.typed_approvals = true,
                PANE_SNAPSHOTS_V1 => caps.pane_snapshots = true,
                SESSION_WORKSPACE_CWD_V1 => caps.session_workspace_cwd = true,
                _ => {}
            }
            caps.raw.insert(name.to_owned(), Value::Bool(true));
        }
        caps
    }

    /// Parse capabilities from a `SessionOpenedResult`-shaped JSON value.
    /// Accepts either `{ capabilities.supported_features: [..] }` (matches
    /// `UiProtocolCapabilities::supported_features`, octos-core ui_protocol.rs:374)
    /// or a `{ capabilities: { feature_name: bool } }` map. Forward-compat
    /// per protocol contract § Capability negotiation.
    pub fn parse(json: &Value) -> Self {
        let node = json
            .pointer("/capabilities")
            .or_else(|| json.pointer("/result/capabilities"))
            .or_else(|| json.pointer("/opened/capabilities"))
            .unwrap_or(json);
        if let Some(arr) = node.get("supported_features").and_then(|v| v.as_array()) {
            return Self::from_supported_features(arr.iter().filter_map(|v| v.as_str()));
        }
        let mut out = Self::default();
        if let Some(map) = node.as_object() {
            for (k, v) in map {
                let on = v.as_bool().unwrap_or(false);
                if on {
                    match k.as_str() {
                        APPROVAL_TYPED_V1 => out.typed_approvals = true,
                        PANE_SNAPSHOTS_V1 => out.pane_snapshots = true,
                        SESSION_WORKSPACE_CWD_V1 => out.session_workspace_cwd = true,
                        _ => {}
                    }
                }
                out.raw.insert(k.clone(), v.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_from_supported_features_array() {
        let v = json!({
            "capabilities": {
                "supported_features": [
                    "approval.typed.v1",
                    "pane.snapshots.v1",
                    "session.workspace_cwd.v1",
                    "future.v1"
                ]
            }
        });
        let caps = Capabilities::parse(&v);
        assert!(caps.typed_approvals);
        assert!(caps.pane_snapshots);
        assert!(caps.session_workspace_cwd);
        assert!(caps.raw.contains_key("future.v1"));
    }

    #[test]
    fn parse_from_bool_map() {
        let v = json!({
            "capabilities": {
                "approval.typed.v1": true,
                "pane.snapshots.v1": false,
                "session.workspace_cwd.v1": true
            }
        });
        let caps = Capabilities::parse(&v);
        assert!(caps.typed_approvals);
        assert!(!caps.pane_snapshots);
        assert!(caps.session_workspace_cwd);
    }

    #[test]
    fn requested_feature_header_is_deterministic() {
        assert_eq!(
            Capabilities::requested().feature_header_value(),
            "approval.typed.v1, pane.snapshots.v1, session.workspace_cwd.v1"
        );
    }
}
