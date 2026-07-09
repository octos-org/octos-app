//! W08 — LoginScreen.
//!
//! A centered, vertically stacked card with three progressively-revealed
//! sections:
//!
//!   1. **Server discovery** — `Server URL` + `Profile ID` text inputs and a
//!      `Continue` button. Hidden once a server URL is configured (loaded
//!      from `~/.config/octos-app/server.json` on boot, written by
//!      `Continue`).
//!   2. **Email** — `Email` input + `Send code` button. Drives
//!      `POST /api/auth/send-code` (octos-cli `auth_handlers.rs:389`).
//!   3. **Verification code** — `Verification code` input + `Verify` button.
//!      Drives `POST /api/auth/verify` (octos-cli `auth_handlers.rs:543`).
//!
//! On a successful verify the bearer token is written to the OS keychain
//! (`octos_app_store::keychain::store_token`) and the App swaps the active
//! page in the parent `PageFlip` from `login_page` to `home_page`.
//!
//! No custom Rust `Widget` impl: the screen is a plain `View` tree and the
//! state machine lives on `App` (`main.rs`). The `script_mod!` block here
//! only registers the DSL prototype, which `App::script_mod` aggregates into
//! the live tree so `body +: { LoginScreen { … } }` parses.
//!
//! See `workstreams/W08-auth-tenancy.md` § "LoginScreen flow" and
//! `04-IA-AND-NAVIGATION.md` § "LoginScreen" for the design.

use makepad_widgets::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // Field row (TextInput) used inside the login card. Lifts the styling
    // from the chat composer's `input` (main.rs:1038–1067) so colors and
    // glyph fallbacks stay consistent — but trimmed to a single row, smaller
    // height, and a visible glassy chrome (the composer input is
    // chrome-less because the surrounding GlassPanel carries the border).
    let LoginField = TextInput {
        width: Fill
        height: 38
        empty_text: ""
        draw_bg +: {
            color: #x06241DCC
            color_hover: #x0A2D24DD
            color_focus: #x0F362DEE
            color_empty: #x06241DCC
            border_color: #x72E4FF44
            border_color_hover: #x72E4FF66
            border_color_focus: #x72E4FF99
            border_color_empty: #x72E4FF44
            border_size: 1.0
            border_radius: 10.0
        }
        draw_text +: {
            color: #xF3E3C7
            color_empty: #xF3E3C766
            text_style: theme.font_regular {
                line_spacing: theme.font_wdgt_line_spacing
                font_size: 13
                font_family: FontFamily {
                    latin := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                    chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                    symbols := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                    emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                }
            }
        }
    }

    let LoginFieldLabel = Label {
        width: Fill
        height: Fit
        margin: Inset{left: 2 bottom: 2 top: 6}
        draw_text.color: #xCDBF9FCC
        draw_text.text_style.font_size: 11
    }

    // Pill-style action button used by the three login steps. Re-shapes the
    // existing `PillButton` (main.rs:168–182) for full-width inside the
    // card and a slightly louder accent so it reads as the primary action.
    let LoginActionButton = ButtonFlat {
        width: Fill
        height: 40
        align: Align{x: 0.5 y: 0.5}
        padding: Inset{left: 14 right: 14 top: 0 bottom: 0}
        margin: Inset{top: 8}
        draw_text +: {
            color: #x06130F
            text_style +: { font_size: 12 }
        }
        draw_bg +: {
            color: #xF6BE63
            color_hover: #xFFD18A
            border_color: #xFFF0D2AA
            border_size: 1.0
            border_radius: 10.0
        }
    }

    // The LoginScreen is registered as a regular widget prototype so the
    // main DSL can place it as `LoginScreen { … }` inside the parent
    // PageFlip. It is a plain View tree — the state machine lives in
    // `App::handle_actions`, which toggles the `visible` flag on each step
    // container.
    mod.widgets.LoginScreen = View {
        width: Fill
        height: Fill
        flow: Down
        align: Align{x: 0.5 y: 0.5}
        show_bg: true
        // Faint matching backdrop (slightly darker than `app_shell`) so the
        // card pops without a hard contrast jump if we re-enter Login from
        // Home (`Logout` keeps the window content visible underneath).
        draw_bg +: {
            color: #x07181599
        }

        login_card := GlassPanel {
            width: Fill{min: 360 max: 420}
            height: Fit
            new_batch: true
            flow: Down
            padding: Inset{left: 30 top: 28 right: 30 bottom: 26}
            spacing: 8
            draw_bg +: {
                tint_color: #x0B3B31
                tint_alpha: 0.92
                border_color: #x72E4FF
                border_alpha: 0.42
                border_width: 1.0
                corner_radius: 22.0
                halo_color: #x72E4FF
                halo_strength: 0.18
                halo_radius: 9.0
                highlight_strength: 0.32
                highlight_band_height: 60.0
                chroma_strength: 0.0
                noise_strength: 0.005
            }

            login_title := Label {
                width: Fill
                height: Fit
                margin: Inset{bottom: 4}
                align: Align{x: 0.5}
                draw_text.color: #xF3E3C7
                draw_text.text_style.font_size: 22
                draw_text.text_style.line_spacing: 1.2
                text: "Octos"
            }

            login_subtitle := Label {
                width: Fill
                height: Fit
                margin: Inset{bottom: 12}
                align: Align{x: 0.5}
                draw_text.color: #xCDBF9FAA
                draw_text.text_style.font_size: 12
                text: "Sign in to your Octos server"
            }

            // Step 1 — Server URL + Profile ID. Hidden once
            // `~/.config/octos-app/server.json` exists.
            login_server_step := View {
                width: Fill
                height: Fit
                flow: Down

                LoginFieldLabel { text: "Server URL" }
                login_server_url_input := LoginField {
                    empty_text: "https://octos.example.com"
                }

                LoginFieldLabel { text: "Profile ID" }
                login_profile_id_input := LoginField {
                    empty_text: "acme"
                }

                login_continue_button := LoginActionButton {
                    text: "Continue"
                }

                Label {
                    width: Fill
                    height: Fit
                    margin: Inset{top: 8}
                    align: Align{x: 0.5}
                    draw_text.color: #xCDBF9F77
                    draw_text.text_style.font_size: 10
                    text: "Tip: if you sign in at acme.octos.ominix.io your Profile ID is `acme`."
                }
            }

            // Step 2 — Email + Send code. Always visible after server step.
            login_email_step := View {
                width: Fill
                height: Fit
                flow: Down

                LoginFieldLabel { text: "Email" }
                login_email_input := LoginField {
                    empty_text: "you@example.com"
                }

                login_send_code_button := LoginActionButton {
                    text: "Send code"
                }
            }

            // Step 3 — Verification code + Verify. Hidden until `Send code`
            // succeeds (server-side advance is unconditional per
            // `auth_handlers.rs:389`, so this flips on as long as the network
            // call returned without a transport error).
            login_code_step := View {
                width: Fill
                height: Fit
                flow: Down
                visible: false

                LoginFieldLabel { text: "Verification code" }
                login_code_input := LoginField {
                    empty_text: "123456"
                }

                login_verify_button := LoginActionButton {
                    text: "Verify"
                }
            }

            // Status / error label. Updated from `App::handle_actions`.
            login_status_label := Label {
                width: Fill
                height: Fit
                margin: Inset{top: 12}
                align: Align{x: 0.5}
                draw_text.color: #xF6BE63
                draw_text.text_style.font_size: 11
                text: ""
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Server-config persistence (`~/.config/octos-app/server.json`).
// ---------------------------------------------------------------------------
//
// Substitute for octos-web's subdomain inference (W08 design § Profile
// resolution mode 2). On a packaged native app there's no host to parse, so
// the first-run dialog asks for both server URL and profile id explicitly,
// and we drop a tiny JSON file alongside other user config so a second
// launch boots straight to the email step.

/// Returns `<config_dir>/octos-app/server.json`. On macOS this resolves to
/// `~/Library/Application Support/octos-app/server.json`; on Linux to
/// `~/.config/octos-app/server.json`. Falls back to `~/.octos-app/` if
/// `dirs`-style discovery fails.
fn server_config_path() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        // Match the W08 spec wording (`~/.config/octos-app/server.json`)
        // verbatim — XDG dirs aren't worth a new dep here, and macOS users
        // already use this layout for many CLI tools.
        p.push(".config");
        p.push("octos-app");
        return Some(p.join("server.json"));
    }
    None
}

/// On-disk shape of the server config file. Kept in this module so we don't
/// thread a dedicated crate boundary just for two strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub server_url: String,
    pub profile_id: String,
}

/// Read `~/.config/octos-app/server.json` if it exists. Returns `None` for
/// "no config yet" (which is the new-user path) and logs+returns `None` on
/// parse errors so a corrupt file doesn't brick the app.
pub fn load_server_config() -> Option<ServerConfig> {
    let path = server_config_path()?;
    let bytes = std::fs::read(&path).ok()?;
    match serde_json::from_slice::<ServerConfig>(&bytes) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            log::warn!("failed to parse {}: {e}", path.display());
            None
        }
    }
}

/// Write `~/.config/octos-app/server.json`. Creates the parent directory if
/// needed. Returns the path written so callers can surface it on error.
pub fn save_server_config(cfg: &ServerConfig) -> std::io::Result<PathBuf> {
    let path = server_config_path()
        .ok_or_else(|| std::io::Error::other("HOME is unset; cannot write server config"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_vec_pretty(cfg)
        .map_err(|e| std::io::Error::other(format!("serialize server config: {e}")))?;
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Non-UI provisioning entry: parse `base_url|profile_id|token` (token
/// optional) and persist the server config + bearer in one shot. Today this
/// is fed by the `makepad.APP_CONFIG` launch-intent extra on Android; a QR
/// onboarding screen can decode a scanned payload into the same call.
pub fn apply_provision_string(prov: &str) -> Result<(), String> {
    let mut parts = prov.trim().splitn(3, '|');
    let url_str = parts.next().unwrap_or("");
    let profile = parts.next().unwrap_or("").trim();
    let token = parts.next().unwrap_or("").trim();
    let url = validate_server_url(url_str)?;
    if profile.is_empty() {
        return Err("provision: profile id missing (want base_url|profile|token)".into());
    }
    save_server_config(&ServerConfig {
        server_url: url.to_string(),
        profile_id: profile.to_string(),
    })
    .map_err(|e| format!("provision: save config: {e}"))?;
    if !token.is_empty() {
        let host = octos_app_store::auth::ServerHost::from(host_from_url(&url));
        let pid = octos_app_store::auth::ProfileId::from(profile.to_string());
        let secret = octos_app_store::auth::SecretToken::from(token.to_string());
        octos_app_store::keychain::store_token(&host, &pid, &secret)
            .map_err(|e| format!("provision: store token: {e}"))?;
    }
    log::info!("provisioned profile `{profile}` @ {url}");
    Ok(())
}

/// Cheap URL validation for the Step 1 input. Accepts `http://` and
/// `https://`; surfaces a one-line error suitable for the status label.
pub fn validate_server_url(s: &str) -> Result<url::Url, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("Server URL is required".to_string());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|e| format!("Invalid server URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("Server URL scheme must be http or https (got `{other}`)")),
    }
    if parsed.host_str().is_none() {
        return Err("Server URL must include a host".to_string());
    }
    Ok(parsed)
}

/// Extract the host portion (no port, no scheme) from a URL — the keychain
/// service name uses this as the first segment so multi-server M2 work can
/// enumerate by prefix.
pub fn host_from_url(u: &url::Url) -> String {
    u.host_str().unwrap_or("unknown-host").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_server_url_accepts_https() {
        let u = validate_server_url("https://octos.example.com").unwrap();
        assert_eq!(u.scheme(), "https");
        assert_eq!(host_from_url(&u), "octos.example.com");
    }

    #[test]
    fn validate_server_url_rejects_empty() {
        assert!(validate_server_url("").is_err());
        assert!(validate_server_url("   ").is_err());
    }

    #[test]
    fn validate_server_url_rejects_unknown_scheme() {
        let e = validate_server_url("ftp://octos.example.com").unwrap_err();
        assert!(e.contains("scheme"));
    }

    #[test]
    fn validate_server_url_rejects_garbage() {
        assert!(validate_server_url("not a url").is_err());
    }

    #[test]
    fn server_config_round_trips_via_serde_json() {
        let cfg = ServerConfig {
            server_url: "https://octos.example.com".to_string(),
            profile_id: "acme".to_string(),
        };
        let bytes = serde_json::to_vec(&cfg).unwrap();
        let back: ServerConfig = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.server_url, cfg.server_url);
        assert_eq!(back.profile_id, cfg.profile_id);
    }
}
