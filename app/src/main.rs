pub use makepad_code_editor;
// Linking the kit activates its `script_mod!` block, which registers
// `mod.widgets.DiagramView`. Without this `pub use`, the DSL can't resolve
// the template below.
pub use makepad_diagram_kit;
pub use makepad_widgets;

mod app;
mod backend;

use makepad_ai::*;
use makepad_widgets::makepad_draw::svg::{
    collect_edges, collect_text_cmds, parse_svg, SvgDocument, SvgEdge, SvgTextAnchor, SvgTextCmd,
};
// `makepad_micro_serde` was used by the dropped flat-file persistence layer;
// W04 will reintroduce it (or `serde_json`) for the SQLite cache.
use makepad_widgets::*;
use octos_app_store::auth::ProfileId;
use octos_app_transport::{
    Capabilities, ProfileId as TransportProfileId, SecretString, StdioSpawn, TransportConfig,
};
use streaming_markdown_kit::{
    streaming_display_with_latex_autowrap_remend, wrap_bare_latex, SanitizeOptions,
};

use crate::backend::OctosUiAgent;

/// Octos profiles supply system prompts server-side, so the client ships an
/// empty placeholder. Replaces aichat's `BackendType::system_prompt` (and the
/// `splash.md` `include_str!`) which baked huge LLM-shaped diagram preambles
/// into the client. See `05-AICHAT-REUSE-MAP.md` "Stuff we drop or replace".
const OCTOS_PLACEHOLDER_SYSTEM_PROMPT: &str = "";

/// The Makepad Splash scripting manual, baked into the client. When "Splash"
/// mode is on, this is prepended to the user's message so the LLM emits a
/// ```runsplash fenced block that the Markdown widget renders as live,
/// clickable UI (see `app_splash_prompt`). Mirrors aichat's
/// `app_generation_session_system_prompt`, but delivered per-message because
/// octos serves system prompts server-side and the protocol carries no
/// client system-prompt field.
const SPLASH_MANUAL: &str = include_str!("../../splash.md");

/// Build the message actually sent to the LLM in Splash mode: instructions +
/// the Splash manual + the user's request. The chat bubble still shows only
/// the user's original `request` text.
fn app_splash_prompt(request: &str) -> String {
    format!(
        "You are a UI-generation agent. Respond with EXACTLY ONE ```runsplash \
fenced code block containing Makepad Splash syntax — no prose before, \
between, or after it, and no other fenced blocks.\n\n\
Hard rules:\n\
- `use mod.prelude.widgets.*` is auto-prepended; do NOT write imports.\n\
- NAME the card: the FIRST line inside the block is `// name: <short-kebab-slug>` \
(a unique, descriptive, STABLE id — e.g. `weather-sf`, `stocks-watchlist`). It is \
stripped before rendering. If you are refining a card from YOUR SAVED CARDS below, \
REUSE its exact same name.\n\
- Do NOT wrap output in Root{{}} or Window{{}}; it is inserted into an \
existing container.\n\
- Interactivity + state: each card has its OWN independent state (keys you \
choose). Read a value with `{{{{state.<key>}}}}` inside a string; change it \
from a button. Events: `inc`/`dec`/`reset` adjust a NUMERIC key, `set` stores a \
string. The payload names the key (default key is `count`):\n\
    Button{{ text: \"+1\" on_click: || agent.notify(\"inc\", {{key: \"count\"}}) }}\n\
    Label{{ text: \"Count: {{{{state.count}}}}\" }}\n\
    Button{{ text: \"Happy\" on_click: || agent.notify(\"set\", {{key: \"mood\", value: \"happy\"}}) }}\n\
- Internet images: fetch a remote picture with `http_resource` in an Image \
widget (downloads asynchronously, appears when ready). Use a real, \
publicly-reachable HTTPS URL (png/jpg/webp/svg):\n\
    Image{{ src: http_resource(\"https://picsum.photos/400/240\") fit: ImageFit.Smallest width: Fill height: 180 }}\n\
  For a REFRESHABLE image, bake the base URL literally and vary ONLY a \
cache-buster query param bound to a counter, plus a button that increments it \
— each tap loads a new picture (never put `{{{{state.*}}}}` as the WHOLE url):\n\
    Image{{ src: http_resource(\"https://picsum.photos/400/240?sig={{{{state.count}}}}\") fit: ImageFit.Smallest width: Fill height: 180 }}\n\
    Button{{ text: \"New Photo\" on_click: || agent.notify(\"inc\", {{}}) }}\n\
- IMMERSIVE FULL-SCREEN WEATHER/PLACE CARD (iOS lock-screen style — the DEFAULT for \
weather, places, travel): a REAL photo of the place FILLS the whole screen (9:16) \
with the text overlaid at the bottom over a dark gradient. Use this EXACT structure — \
an Overlay of image, then dark scrim, then text pinned to the bottom:\n\
    View{{ width: Fill height: 700 flow: Overlay\n\
        Image{{ src: http_resource(\"https://loremflickr.com/1080/1920/tokyo,skyline,cityscape\") fit: ImageFit.CropToFill width: Fill height: Fill }}\n\
        GradientYView{{ width: Fill height: Fill draw_bg.color: #00000000 draw_bg.color_2: #000000E6 }}\n\
        View{{ width: Fill height: Fill flow: Down align: {{x: 0.0 y: 1.0}} padding: Inset{{left: 28 right: 28 bottom: 64}}\n\
            Label{{ text: \"Tokyo\" draw_text.color: #ffffff draw_text.text_style.font_size: 26 }}\n\
            Label{{ text: \"72°\" draw_text.color: #ffffff draw_text.text_style.font_size: 88 margin: Inset{{top: 8 bottom: 4}} }}\n\
            Label{{ text: \"Sunny\" draw_text.color: #ffffff draw_text.text_style.font_size: 19 }}\n\
            Label{{ text: \"H:78°  L:64°\" draw_text.color: #ffffffcc draw_text.text_style.font_size: 16 }}\n\
        }}\n\
    }}\n\
  REAL IMAGE: `https://loremflickr.com/1080/1920/<city>,skyline,cityscape` returns a \
real Flickr photo of that place (comma-separated keywords, url-encoded). Add \
`?lock=<n>` to lock one stable image (e.g. `.../1080/1920/paris,eiffel?lock=7`). The \
GradientYView is a dark scrim (transparent top -> dark bottom) so the WHITE text \
stays readable over ANY photo. Keep `height: 700` (fills the whole screen), and put \
ALL text in the BOTTOM overlay (align y: 1.0). The hero temperature keeps its margin.\n\
- Keep it self-contained and visually clean (padding, spacing, rounded \
containers, readable labels).\n\
- CRITICAL OVERRIDE (takes precedence over the manual's `let` examples): the \
block MUST BEGIN DIRECTLY with a single root container widget — e.g. \
`RoundedView{{` or `View{{`. Do NOT start with, or use, any top-level `let \
X = …` component definitions. Inline/repeat any shared structure directly, \
even if it makes the output longer. A leading `let` will fail to render.\n\
- NO custom shaders/MPSL: never write `pixel: fn`, `fn(`, `let`, `mut`, `Sdf2d`, \
`uniform(`, `instance(`, or `.mix(` inside `draw_bg` — they crash the WHOLE card \
into ugly raw source. WIDGET-PROPERTY RULES (setting a property a widget does not \
have ALSO crashes the card): a ROUNDED card is \
`RoundedView{{ draw_bg.color: #hex draw_bg.border_radius: 20.0 }}` (solid fill, \
supports border_radius). A GRADIENT is \
`GradientYView{{ draw_bg.color: #topHex draw_bg.color_2: #botHex }}` (vertical; \
`GradientXView` = horizontal) — it is a full-width RECTANGLE and has NO \
border_radius, so NEVER put `border_radius` on a Gradient*View. Pick one per \
container; don't mix. Style ONLY with: draw_bg.color, draw_bg.color_2 \
(gradient views only), draw_bg.border_radius (rounded views only), \
draw_text.color, draw_text.text_style.font_size.\n\
- iOS REFINEMENT (make it look like a real iOS app): prefer \
`RoundedShadowView{{ draw_bg.color: #hex draw_bg.border_radius: 24.0 draw_bg.shadow_color: #00000055 draw_bg.shadow_offset: vec2(0.0, 8.0) draw_bg.shadow_radius: 24.0 margin: 14 }}` \
as the CARD container — rounded corners + a soft iOS drop shadow (it DOES support \
border_radius; keep a `margin` so the shadow has room). WRAP long text: any \
headline/sentence Label MUST set `width: Fill` so it wraps to multiple lines instead \
of clipping. Size hierarchy via font_size: hero value 52-72 (a very large number like a \
temperature MUST have `margin: Inset{{top: 10 bottom: 6}}` and its OWN line, or \
its tall glyph tops get clipped by the label above it), title 16-18, row 15, \
caption 12-13; make secondary text translucent `draw_text.color: #ffffff99` (or \
`#8e8e93` on light cards). Hairline row dividers: \
`SolidView{{ width: Fill height: 1 draw_bg.color: #ffffff14 }}`. iOS system colors: \
blue #0a84ff, red #ff453a, green #32d74b, dark card #1c1c1e, light card #f2f2f7. \
Generous, consistent padding (18-24) and spacing (10-14).\n\
- LIVE DATA: you may fetch real data with a web tool, but it reliably returns only \
SIMPLE single-endpoint sources — e.g. weather `https://wttr.in/<City>?format=j1`. \
Multi-request or big-JSON APIs (stock quotes, news lists) usually FAIL; if the user did \
not supply those numbers, ask for them — never invent live prices or headlines.\n\
- ITERATE: if the user asks to refine a card you built earlier in this chat, reuse its \
structure and change only what they asked; still exactly one runsplash block.\n\n\
Follow this Splash manual EXACTLY (except the overrides above):\n\n{manual}\n\n\
User request: {request}",
        manual = SPLASH_MANUAL,
        request = request,
    )
}

/// Per-card A2App/Splash state: `{{state.<key>}}` key → value. Each rendered
/// card owns one of these (keyed by message index in `CHAT_DATA.a2app_state`)
/// so independent cards never share state.
type CardState = std::collections::BTreeMap<String, String>;

/// Tag every `agent.notify("<ev>"` / `agent.notify('<ev>'` in a Splash body with
/// the owning card's id → `agent.notify("<item_id>:<ev>"`. The framework's
/// `SplashAction::Notify` carries no source card, so this prefix is how a button
/// press is routed back to the card that fired it (per-card state isolation).
fn tag_notify_calls(body: &str, item_id: usize) -> String {
    if !body.contains("agent.notify(") {
        return body.to_string();
    }
    body.replace("agent.notify(\"", &format!("agent.notify(\"{item_id}:"))
        .replace("agent.notify('", &format!("agent.notify('{item_id}:"))
}

/// Rewrite a bare `View{` — the transparent layout container LLMs reach for —
/// into `SolidView{show_bg: false `. A bare `View{` crashes the Splash eval,
/// which dumps the WHOLE card as raw source instead of UI; a `SolidView` with
/// its background disabled is an equivalent invisible layout container that
/// renders. Only rewrites `View{` NOT preceded by an ASCII letter, so
/// `RoundedView{`, `SolidView{`, `GradientYView{`, `ScrollXView{`, … stay intact.
fn neutralize_bare_view(body: &str) -> String {
    if !body.contains("View{") {
        return body.to_string();
    }
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len() + 32);
    let mut last = 0;
    let mut search = 0;
    while let Some(rel) = body[search..].find("View{") {
        let pos = search + rel;
        if pos > 0 && bytes[pos - 1].is_ascii_alphabetic() {
            // part of a longer widget name (RoundedView, SolidView, …) — skip
            search = pos + "View{".len();
            continue;
        }
        out.push_str(&body[last..pos]);
        out.push_str("SolidView{show_bg: false ");
        last = pos + "View{".len();
        search = last;
    }
    out.push_str(&body[last..]);
    out
}

/// Substitute `{{state.<key>}}` tokens with this card's live values. Missing
/// keys render `"0"` (keeps counter cards reading 0 before any interaction, and
/// is a safe default for a not-yet-set string).
fn substitute_state_keys(text: &str, state: &CardState) -> String {
    if !text.contains("{{state.") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("{{state.") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + "{{state.".len()..];
        if let Some(end) = after.find("}}") {
            let key = after[..end].trim();
            out.push_str(state.get(key).map(String::as_str).unwrap_or("0"));
            rest = &after[end + 2..];
        } else {
            out.push_str(&rest[pos..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Persistent registry of named A2App cards, so a card can be retrieved by
/// name and refined/improved over time (`$HOME` is the app-private files dir
/// on Android; see `set_var("HOME", get_data_dir())` at startup).
fn a2app_cards_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::Path::new(&home).join("a2app_cards"))
}

/// Extract the `// name: <slug>` directive the model puts on the FIRST line of a
/// card body. Sanitized to a stable kebab slug so it names a file safely.
fn extract_card_name(body: &str) -> Option<String> {
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("// name:").or_else(|| t.strip_prefix("//name:")) {
            let slug: String = rest
                .trim()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                .collect();
            let slug = slug.trim_matches('-').to_string();
            if !slug.is_empty() {
                return Some(slug.chars().take(48).collect());
            }
        }
        // The directive lives at the very top; stop once real widget code starts.
        if t.starts_with(|c: char| c.is_ascii_uppercase()) {
            break;
        }
    }
    None
}

/// Drop the `// name:` directive line before the body reaches the Splash VM
/// (which does not accept `//` line comments — leaving it in crashes the card).
/// Only matches a line whose trimmed text starts with `// name:`, so URLs
/// containing `//` inside strings are untouched.
fn strip_card_name_line(body: &str) -> std::borrow::Cow<'_, str> {
    if !body.contains("// name:") && !body.contains("//name:") {
        return std::borrow::Cow::Borrowed(body);
    }
    let kept: Vec<&str> = body
        .lines()
        .filter(|l| {
            let t = l.trim();
            !(t.starts_with("// name:") || t.starts_with("//name:"))
        })
        .collect();
    std::borrow::Cow::Owned(kept.join("\n"))
}

/// Persist a named card's runsplash DSL (with its `// name:` line) for reuse.
fn save_a2app_card(name: &str, dsl: &str) {
    if let Some(dir) = a2app_cards_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{name}.splash"));
        match std::fs::write(&path, dsl) {
            Ok(()) => log::info!("a2app: saved card '{name}' ({} bytes) → {}", dsl.len(), path.display()),
            Err(e) => log::warn!("a2app: save card '{name}' failed: {e}"),
        }
    } else {
        log::warn!("a2app: cannot save card '{name}' — no HOME/cards dir");
    }
}

/// Load saved cards as `(name, dsl)`, newest-modified first, capped at `max`.
fn load_a2app_cards(max: usize) -> Vec<(String, String)> {
    let Some(dir) = a2app_cards_dir() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut entries: Vec<(std::time::SystemTime, String, String)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("splash") {
            continue;
        }
        let name = p.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
        let mtime = e
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if let Ok(dsl) = std::fs::read_to_string(&p) {
            if !name.is_empty() {
                entries.push((mtime, name, dsl));
            }
        }
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().take(max).map(|(_, n, d)| (n, d)).collect()
}

/// Prepare a *raw* Splash body for a specific card: drop the `// name:`
/// directive, substitute its state values, neutralize bare `View{}`, and tag
/// its notify calls with the card id.
fn substitute_card_state(body: &str, item_id: usize, state: &CardState) -> String {
    let named = strip_card_name_line(body);
    let subst = substitute_state_keys(&named, state);
    let safe = neutralize_bare_view(&subst);
    tag_notify_calls(&safe, item_id)
}

/// Whole-message variant: substitute `{{state.*}}` and tag notify calls ONLY
/// inside ```runsplash fenced blocks (the generated live UI). Ordinary prose or
/// other code fences are left verbatim — they're the model's own text, not live
/// state, and rewriting them was a bug (`{{state.count}}` in an explanation
/// became `0`). No-op for normal messages: no runsplash block ⇒ nothing to do.
fn resolve_a2app_card(text: &str, item_id: usize, state: &CardState) -> String {
    if !text.contains("```runsplash") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("```runsplash") {
        // Copy up to and including the opening fence line verbatim.
        let after_marker = open + "```runsplash".len();
        let line_end = match rest[after_marker..].find('\n') {
            Some(nl) => after_marker + nl + 1,
            None => rest.len(),
        };
        out.push_str(&rest[..line_end]);
        let body_and_rest = &rest[line_end..];
        // Body runs to the closing fence; process only within it. The closing
        // ``` is copied verbatim by the next iteration's prefix (or the trailing
        // push below).
        match body_and_rest.find("```") {
            Some(close) => {
                out.push_str(&substitute_card_state(&body_and_rest[..close], item_id, state));
                rest = &body_and_rest[close..];
            }
            None => {
                out.push_str(&substitute_card_state(body_and_rest, item_id, state));
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// While a reply is still streaming, hold back an UNCLOSED ```runsplash
/// block: the downstream remend pass auto-closes open fences, which would
/// dispatch every partial body to the Splash widget — a full script-VM eval
/// per repaint (observed ~60 evals for one card) and a jittering half-built
/// layout. Instead, cut the text at the open fence and show a small building
/// note; the card renders exactly once when the closing fence arrives.
fn defer_unclosed_runsplash(text: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let Some(start) = text.rfind("```runsplash") else {
        return Cow::Borrowed(text);
    };
    let after = &text[start + "```runsplash".len()..];
    let closed = match after.find('\n') {
        // Fence body present — closed iff a terminating ``` follows.
        Some(nl) => after[nl + 1..].contains("```"),
        // Mid-fence-line — certainly not closed yet.
        None => false,
    };
    if closed {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(format!("{}\u{1F6E0} Building app UI\u{2026}", &text[..start]))
    }
}

/// Pull the body of the first ```runsplash fenced block out of a message so
/// it can be fed straight to a `Splash` widget. Returns the raw Splash script
/// (still containing any `{{state.*}}` placeholders).
fn extract_runsplash_body(text: &str) -> Option<&str> {
    let start = text.find("```runsplash")?;
    let after = &text[start + "```runsplash".len()..];
    let body_start = after.find('\n')? + 1;
    let body = &after[body_start..];
    let end = body.find("```")?;
    Some(body[..end].trim_end())
}

/// Short A2App directive for follow-up requests in a session that already has
/// the Splash manual in its history (see `App::splash_primed`). Avoids
/// re-sending the ~85KB manual every turn.
fn app_splash_followup(request: &str) -> String {
    format!(
        "Respond with EXACTLY ONE ```runsplash fenced block (Makepad Splash \
syntax, no prose, no other fences), following the Splash manual already \
provided earlier in this conversation. Same rules: no imports, no \
Root/Window wrapper. FIRST line inside the block = `// name: <slug>` (reuse the \
same name when refining one of YOUR SAVED CARDS below). Each card has its OWN \
state: read `{{{{state.<key>}}}}`; \
change it with `agent.notify(\"inc\"/\"dec\"/\"reset\", {{key: \"count\"}})` for \
numbers or `agent.notify(\"set\", {{key, value}})` for strings. Internet images: \
`Image{{ src: http_resource(\"https://…\") fit: ImageFit.Smallest }}`; refreshable \
= cache-buster `?sig={{{{state.count}}}}` + a button that does `inc`. CRITICAL: begin DIRECTLY with \
a single root container widget (e.g. `RoundedView{{`) — NO top-level `let X = \
…` component definitions (inline/repeat instead); a leading `let` fails to \
render.\n\nUser request: {request}",
    )
}

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.CodeView
    use mod.widgets.DiagramView
    use mod.text.*
    use mod.res.*
    use mod.draw.*

    // Override theme fonts. Two purposes:
    //   1. font_code — CJK-capable monospace (LXGW Mono) so `` `inline` ``
    //      and CodeView render Chinese correctly.
    //   2. font_regular — add a symbols-capable latin (NotoSans) so Unicode
    //      blocks outside IBM Plex Sans's repertoire (arrows U+2190-U+21FF,
    //      math operators, misc technical) render as glyphs instead of tofu.
    //
    // Note: Makepad's Markdown widget bakes `theme.font_*` at expansion time,
    // so these theme-level overrides are necessary but not sufficient —
    // per-instance overrides on each Markdown instance are also applied below.
    mod.themes.dark = mod.themes.dark{
        font_code: TextStyle{
            font_size: theme.font_size_code
            font_family: FontFamily{
                latin := FontMember{res: crate_resource("self:resources/LiberationMono-Regular.ttf") asc: 0.0 desc: 0.0}
                chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                symbols := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
            }
            line_spacing: 1.35
        }
        font_regular: mod.themes.dark.font_regular{
            font_family: FontFamily{
                latin := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                symbols := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
            }
        }
    }

    let ai_ink = #x06130F
    let ai_panel = #x0A3A30
    let ai_panel_deep = #x06251F
    let ai_cream = #xF3E3C7
    let ai_cream_dim = #xE0D2BACC
    let ai_cyan = #x72E4FF
    let ai_cyan_soft = #x72E4FF77
    let ai_gold = #xF6BE63
    let ai_gold_soft = #xF6BE6388

    let chat_scene_bg = Gradient{x1: 0 y1: 0 x2: 1 y2: 1
        Stop{offset: 0 color: #x071018 opacity: 0.56}
        Stop{offset: 0.44 color: #x121923 opacity: 0.52}
        Stop{offset: 0.72 color: #x18202B opacity: 0.48}
        Stop{offset: 1 color: #x201722 opacity: 0.52}
    }

    let chat_scene_cyan = RadGradient{cx: 0.14 cy: 0.16 r: 0.44
        Stop{offset: 0 color: #x72E4FF opacity: 0.70}
        Stop{offset: 0.44 color: #x2E84FF opacity: 0.20}
        Stop{offset: 1 color: #x2E84FF opacity: 0.0}
    }

    let chat_scene_gold = RadGradient{cx: 0.88 cy: 0.14 r: 0.36
        Stop{offset: 0 color: #xFFD18A opacity: 0.52}
        Stop{offset: 0.50 color: #xFF8F3A opacity: 0.15}
        Stop{offset: 1 color: #xFF8F3A opacity: 0.0}
    }

    let chat_scene_violet = RadGradient{cx: 0.64 cy: 0.88 r: 0.48
        Stop{offset: 0 color: #xDCA5FF opacity: 0.48}
        Stop{offset: 0.54 color: #x806DFF opacity: 0.14}
        Stop{offset: 1 color: #x806DFF opacity: 0.0}
    }

    let chat_scene_mint = RadGradient{cx: 0.28 cy: 0.76 r: 0.38
        Stop{offset: 0 color: #x8AFFD1 opacity: 0.42}
        Stop{offset: 0.48 color: #x2BD7B7 opacity: 0.12}
        Stop{offset: 1 color: #x2BD7B7 opacity: 0.0}
    }

    let ChatSceneVector = Vector{
        width: Fill
        height: Fill
        viewbox: vec4(0 0 1200 820)

        Rect{x: 0 y: 0 w: 1200 h: 820 fill: chat_scene_bg}
        Circle{cx: 160 cy: 112 r: 350 fill: chat_scene_cyan}
        Circle{cx: 1080 cy: 112 r: 290 fill: chat_scene_gold}
        Circle{cx: 768 cy: 790 r: 390 fill: chat_scene_violet}
        Circle{cx: 320 cy: 650 r: 300 fill: chat_scene_mint}

        Rect{x: 24 y: 28 w: 1152 h: 760 rx: 38 ry: 38 fill: #x07101822}
        Rect{x: 24 y: 28 w: 1152 h: 760 rx: 38 ry: 38 fill: false stroke: #xFFFFFF1A stroke_width: 1.2}
        Rect{x: 28 y: 32 w: 1144 h: 752 rx: 36 ry: 36 fill: false stroke: #x72E4FF20 stroke_width: 1.0}
        Rect{x: 42 y: 44 w: 1116 h: 724 rx: 32 ry: 32 fill: false stroke: #xFFD18A10 stroke_width: 0.8}

        Path{d: "M -80 190 C 170 72 330 120 520 70 S 905 20 1280 110" fill: false stroke: #x72E4FF22 stroke_width: 2.6 stroke_linecap: "round"}
        Path{d: "M -60 610 C 160 500 348 548 548 480 S 900 380 1260 475" fill: false stroke: #xDCA5FF1E stroke_width: 2.2 stroke_linecap: "round"}
        Path{d: "M 1120 -40 C 960 156 900 286 730 374 S 470 528 248 878" fill: false stroke: #xFFD18A1A stroke_width: 2.0 stroke_linecap: "round"}

        Rect{x: 92 y: 74 w: 320 h: 118 rx: 34 ry: 34 fill: #xFFFFFF05}
        Rect{x: 850 y: 84 w: 244 h: 88 rx: 30 ry: 30 fill: #xFFFFFF06}
        Rect{x: 470 y: 612 w: 330 h: 118 rx: 34 ry: 34 fill: #xFFFFFF05}
    }

    let ToolbarLabel = Label {
        draw_text.color: ai_cream_dim
        draw_text.text_style.font_size: 11
    }

    let ToolbarGlass = GlassPanel {
        height: 38
        flow: Right
        align: Align{y: 0.5}
        spacing: 8
        padding: Inset{left: 12 right: 12 top: 0 bottom: 0}
        draw_bg +: {
            tint_color: #x06231C
            tint_alpha: 0.88
            border_color: #x72E4FF
            border_alpha: 0.24
            border_width: 1.0
            corner_radius: 14.0
            halo_strength: 0.0
            halo_radius: 0.0
            highlight_strength: 0.10
            highlight_band_height: 18.0
            noise_strength: 0.003
        }
    }

    let PillButton = ButtonFlat {
        height: 34
        padding: Inset{left: 14 right: 14 top: 0 bottom: 0}
        draw_text +: {
            color: ai_cream
            text_style +: { font_size: 11 }
        }
        draw_bg +: {
            color: #x08251EB8
            color_hover: #x123B31DD
            border_color: #xEAD8B82D
            border_size: 1.0
            border_radius: 10.0
        }
    }

    let IconButton = ButtonFlat {
        width: 36
        height: 36
        padding: 0
        draw_text +: {
            color: ai_cream
            text_style +: { font_size: 15 }
        }
        draw_bg +: {
            color: #x08251EB0
            color_hover: #x154337DD
            border_color: #xEAD8B82A
            border_size: 1.0
            border_radius: 10.0
        }
    }

    let SendButton = ButtonFlatIcon {
        width: 36
        height: 36
        padding: 0
        icon_walk: Walk{ width: 20, height: 20 }
        draw_icon +: {
            color: ai_gold
            svg: crate_resource("self:resources/icons/send.svg")
        }
        // Flat icon button — no filled circle behind the send glyph.
        draw_bg +: {
            color: #00000000
            color_hover: #xEAD8B814
            border_size: 0.0
            border_radius: 8.0
        }
    }

    let GlassSlider = SliderMinimal {
        width: 170
        height: 28
        text: ""
        min: 0.72
        max: 0.98
        step: 0.01
        default: 0.90
        precision: 2
        label_walk: Walk{width: 0 height: 0}
        text_input: TextInput{
            width: 0
            height: 0
            is_read_only: true
        }
        draw_bg +: {
            hover: instance(0.0)
            focus: instance(0.0)
            drag: instance(0.0)
            disabled: instance(0.0)
            border_size: 0.0
            offset_y: 11.0
            handle_size: 20.0
            color: #x9CC9C24A
            color_hover: #x9CC9C266
            color_focus: #x9CC9C266
            color_drag: #x9CC9C280
            color_2: #x0A241EAA
            color_2_hover: #x0E3028CC
            color_2_focus: #x0E3028CC
            color_2_drag: #x123C32DD
            val_color: ai_gold
            val_color_hover: #xFFD98B
            val_color_focus: #xFFD98B
            val_color_drag: #xFFE2A3
            handle_color: ai_gold
            handle_color_hover: #xFFF0D2
            handle_color_focus: #xFFF0D2
            handle_color_drag: #xFFF0D2
            border_color: #x72E4FF44
            border_color_2: #x00000055
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let track_y = self.rect_size.y * 0.5 - 2.0
                let track_h = 4.0
                let handle_x = clamp(
                    self.slide_pos * self.rect_size.x,
                    8.0,
                    self.rect_size.x - 8.0
                )
                let handle_r = 8.0 + self.hover * 1.0

                sdf.box(0.0, track_y, self.rect_size.x, track_h, 2.0)
                sdf.fill(#x6EA99E66)

                sdf.box(0.0, track_y, handle_x, track_h, 2.0)
                sdf.fill(self.val_color.mix(self.val_color_hover, self.hover))

                sdf.circle(handle_x, self.rect_size.y * 0.5, handle_r)
                sdf.fill_keep(self.handle_color.mix(self.handle_color_hover, self.hover))
                sdf.stroke(#xFFF0D288, 1.0)

                return sdf.result
            }
        }
    }

    let MermaidSvgView = #(MermaidSvgView::register_widget(vm)) {
        width: Fill
        height: Fit
        // Animated flow dot shader: SDF circle + halo. Per-edge color
        // (incl. pulse alpha in `.w`) is written from Rust.
        draw_flow_dot +: {
            color: #xe2e8f0
            pixel: fn() {
                let r = length(self.pos - vec2(0.5, 0.5))
                let core = 1.0 - smoothstep(0.30, 0.38, r)
                let halo = (1.0 - smoothstep(0.38, 0.50, r)) * 0.55
                let a = clamp(core + halo, 0.0, 1.0) * self.color.w
                return Pal.premul(vec4(self.color.xyz, a))
            }
        }
        draw_text +: {
            color: #xe2e8f0
            text_style: theme.font_code{
                font_size: 12
                font_family: FontFamily{
                    latin := FontMember{res: crate_resource("self:resources/LiberationMono-Regular.ttf") asc: 0.0 desc: 0.0}
                    chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                    emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                }
            }
        }
    }

    let ChatList = #(ChatList::register_widget(vm)) {
        width: Fill
        height: Fill

        list := PortalList {
            width: Fill
            height: Fill
            flow: Down
            // Touch: finger-drag scrolls the thread. List-level `selectable`
            // is off because it wins over drag on touch (a drag on text
            // enters selection and never scrolls — the reported bug); the
            // per-answer copy icon covers text extraction on mobile.
            drag_scrolling: true
            auto_tail: true
            smooth_tail: true
            selectable: false
            // Hide the right-edge scrollbar (drag-to-scroll is the gesture).
            scroll_bar: mod.widgets.ScrollBar { bar_size: 0.0 }

            User := RoundedView {
                width: Fill
                height: Fit
                // Full page width (was left:50 chat-bubble indent).
                margin: Inset{top: 4 bottom: 4 left: 8 right: 8}
                padding: Inset{left: 12 top: 8 right: 12 bottom: 8}
                flow: Down
                show_bg: true
                draw_bg +: {
                    color: #x0B2A22E6
                    radius: 12.0
                }

                selectable := Markdown {
                    width: Fill
                    height: Fit
                    // Off on mobile: per-widget text selection fought the
                    // list's drag-to-scroll (a swipe popped Android's
                    // Copy/Cut toolbar mid-scroll). Copy icon covers this.
                    selectable: false
                    use_code_block_widget: true
                    use_math_widget: true
                    body: ""
                    // Per-instance override for `` `inline code` ``. The
                    // Markdown widget bakes `theme.font_code` at expansion
                    // time, so a later `mod.themes.dark{...}` override
                    // doesn't reach it. Without this override, CJK inside
                    // backticks renders as tofu (no glyph) because Liberation
                    // Mono is Latin-only.
                    text_style_fixed: theme.font_code{
                        font_family: FontFamily{
                            latin := FontMember{res: crate_resource("self:resources/LiberationMono-Regular.ttf") asc: 0.0 desc: 0.0}
                            chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                            symbols := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                            emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                        }
                    }
                    // Prose font family with symbols fallback — fixes "tofu"
                    // for Unicode arrows / math / misc technical symbols
                    // (observed trigger: `1→5`, `≤`, `≥`, `α` in prose).
                    text_style_normal: theme.font_regular{
                        font_family: FontFamily{
                            latin := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                            chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                            symbols := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                            emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                        }
                    }
                    code_block := ScrollXView {
                        width: Fill
                        height: Fit
                        flow: Right
                        code_view := CodeView {
                            keep_cursor_at_end: false
                            editor +: {
                                height: Fit
                                draw_bg +: { color: #x031510EE }
                            }
                        }
                    }
                    splash_block := View {
                        width: Fill
                        height: Fit
                        splash_view := Splash {
                            width: Fill
                            height: Fit
                        }
                    }
                    // Diagram block — rendered by makepad-diagram-kit's
                    // DiagramView. The inner `diagram_view` id matches what
                    // the markdown widget's `ids!(diagram_view).set_text`
                    // dispatch expects.
                    diagram_block := ScrollXView {
                        width: Fill
                        height: Fit
                        flow: Right
                        diagram_view := DiagramView {
                            width: Fit
                            height: Fit
                        }
                    }
                    mermaid_block := ScrollXView {
                        width: Fill
                        height: Fit
                        flow: Right
                        mermaid_view := MermaidSvgView {
                            width: Fit
                            height: Fit
                        }
                    }
                    inline_math := MathView {
                        // MathView lays out at font_size*1.75; body is ~10,
                        // so 5.7 keeps inline math the same height as text.
                        font_size: 5.7
                    }
                    display_math := MathView {
                        font_size: 6.3
                    }
                }

                // (Per-message close button removed — user directive.)
                View {
                    width: Fill
                    height: Fit
                    align: Align{x: 1.0}
                }
            }

            Assistant := RoundedView {
                width: Fill
                height: Fit
                // Edge-to-edge: no bubble margin/padding/background so the A2App
                // card fills the entire screen (was margin 8 / padding 12 with a
                // dark bubble bg — that framed the card and broke full-screen).
                margin: Inset{top: 0 bottom: 0 left: 0 right: 0}
                padding: Inset{left: 0 top: 0 right: 0 bottom: 0}
                flow: Down
                show_bg: false
                draw_bg +: {
                    color: #x0B2A22E6
                    radius: 0.0
                }

                RubberView {
                    width: Fill
                    height: Fit
                    smoothing: 0.3

                    selectable := Markdown {
                        width: Fill
                        height: Fit
                        // Off on mobile — see User bubble note (drag scrolls,
                        // copy icon extracts).
                        selectable: false
                        use_code_block_widget: true
                        use_math_widget: true
                        body: ""
                        // Per-instance override — same as User's Markdown
                        // above. Fixes `` `中文` `` inline-code tofu.
                        text_style_fixed: theme.font_code{
                            font_family: FontFamily{
                                latin := FontMember{res: crate_resource("self:resources/LiberationMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                            }
                        }
                        draw_text +: {
                            get_color: fn() {
                                let fade_chars = 50.0
                                let dist_from_end = self.total_chars - self.char_index
                                let t = clamp(dist_from_end / fade_chars, 0.0, 1.0)
                                let alpha = pow(t, 0.5)
                                return vec4(self.color.rgb, self.color.a * alpha)
                            }
                        }
                        code_block := ScrollXView {
                            width: Fill
                            height: Fit
                            flow: Right
                            code_view := CodeView {
                                keep_cursor_at_end: true
                                editor +: {
                                    height: Fit
                                    draw_bg +: { color: #x031510EE }
                                    // Local font override: CodeView is defined in the
                                    // makepad-code-editor crate and bakes `theme.font_code`
                                    // at its own expansion time, so later `mod.themes.dark`
                                    // overrides don't reach it. Override per-instance.
                                    draw_text +: {
                                        text_style: theme.font_code{
                                            font_family: FontFamily{
                                                latin := FontMember{res: crate_resource("self:resources/LiberationMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                                chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                                emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                                            }
                                        }
                                    }
                                    draw_gutter +: {
                                        text_style: theme.font_code{
                                            font_family: FontFamily{
                                                latin := FontMember{res: crate_resource("self:resources/LiberationMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                                chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                                emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        splash_block := SolidView{
                            flow: Overlay
                            new_batch: true
                            width: Fill
                            height: Fit
                            splash_view := Splash {
                                flow: Overlay
                                width: Fill
                                height: Fit
                            }
                        }
                        // Diagram block — see User-side comment.
                        diagram_block := ScrollXView{
                            flow: Right
                            new_batch: true
                            width: Fill
                            height: Fit
                            diagram_view := DiagramView {
                                width: Fit
                                height: Fit
                            }
                        }
                        mermaid_block := ScrollXView{
                            flow: Right
                            new_batch: true
                            width: Fill
                            height: Fit
                            mermaid_view := MermaidSvgView {
                                width: Fit
                                height: Fit
                            }
                        }
                        inline_math := MathView {
                            // Match body text height (font_size*1.75 ≈ body).
                            font_size: 5.7
                        }
                        display_math := MathView {
                            font_size: 6.3
                        }
                    }
                }

                // Answer action row: copy + share, drawn natively from the
                // supplied SVGs via each button's DrawSvg icon slot.
                // `draw_icon.color` overrides the SVG `currentColor`. Both are
                // gated off until the answer completes (draw loop hides them
                // on the in-flight item). Flat transparent button bg.
                actions_row := View {
                    width: Fill
                    height: Fit
                    flow: Right
                    align: Align{x: 0.0 y: 0.5}
                    spacing: 2
                    copy_button := ButtonFlatIcon {
                        width: 34
                        height: 30
                        margin: Inset{top: 6 left: 2}
                        icon_walk: Walk{ width: 19, height: 19 }
                        draw_icon +: {
                            color: #xB6C6BE
                            svg: crate_resource("self:resources/icons/copy.svg")
                        }
                        draw_bg +: {
                            color: #00000000
                            color_hover: #xEAD8B814
                            border_size: 0.0
                            border_radius: 8.0
                        }
                    }
                    share_button := ButtonFlatIcon {
                        width: 34
                        height: 30
                        margin: Inset{top: 6}
                        icon_walk: Walk{ width: 19, height: 19 }
                        draw_icon +: {
                            color: #xB6C6BE
                            svg: crate_resource("self:resources/icons/share.svg")
                        }
                        draw_bg +: {
                            color: #00000000
                            color_hover: #xEAD8B814
                            border_size: 0.0
                            border_radius: 8.0
                        }
                    }
                }
            }
        }
    }

    // SessionList — sidebar pane backed by `octos_app_store::SessionMap`.
    // Replaces W02's static `nav_recent` placeholder; see
    // `app/src/app/sessions.rs`. Pattern lifted from
    // `aichat/examples/aichat/src/main.rs:343` (ChatList DSL) and `:1774-1881`
    // (Widget impl); item template models the row design in
    // `04-IA-AND-NAVIGATION.md` § Sidebar with `octos-web/src/components/session-list.tsx`'s
    // hover-x affordance.
    let SessionList = #(crate::app::sessions::SessionList::register_widget(vm)) {
        width: Fill
        height: Fit

        list := PortalList {
            width: Fill
            height: Fill
            flow: Down
            drag_scrolling: true
            auto_tail: false
            selectable: true

            SessionItem := RoundedView {
                width: Fill
                height: Fit
                flow: Right
                margin: Inset{top: 2 bottom: 2 left: 0 right: 0}
                padding: Inset{left: 6 top: 6 right: 6 bottom: 6}
                spacing: 6
                align: Align{y: 0.5}
                show_bg: true
                draw_bg +: {
                    color: #x0A2A2200
                    color_hover: #xEAD8B814
                    radius: 8.0
                }

                // Streaming / active-task dot. Hidden by Rust when neither
                // flag is set; see `octos_app_store::sessions::is_session_active`.
                streaming_dot := Label {
                    width: Fit
                    height: Fit
                    text: "●"
                    margin: Inset{right: 2}
                    draw_text.color: #x72E4FF
                    draw_text.text_style.font_size: 10
                }

                // Selection caret — Rust toggles visibility when this row's
                // id matches `APP_STATE.current_session`.
                selected_marker := Label {
                    width: Fit
                    height: Fit
                    text: "▸"
                    margin: Inset{right: 2}
                    draw_text.color: #xF6BE63
                    draw_text.text_style.font_size: 10
                }

                // The row's click target doubles as its title: Buttons render
                // only their OWN text (child Labels nested inside a Button
                // are never drawn — Button::draw_walk paints bg/icon/text and
                // stops), so `row_click.text` carries the session title,
                // set from `SessionList::draw_walk`.
                row_click := ButtonFlat {
                    width: Fill
                    height: Fit
                    align: Align{x: 0.0 y: 0.5}
                    padding: Inset{left: 2 top: 4 right: 2 bottom: 4}
                    text: ""
                    draw_text +: {
                        color: #xF3E3C7
                        text_style +: { font_size: 12 }
                    }
                    draw_bg +: {
                        color: #00000000
                        color_hover: #xEAD8B810
                        border_size: 0.0
                        border_radius: 6.0
                    }
                }

                delete_button := ButtonFlat {
                    width: Fit
                    height: Fit
                    padding: Inset{top: 2 bottom: 2 left: 6 right: 6}
                    margin: Inset{left: 2}
                    text: "x"
                    draw_text +: {
                        color: #xCDBF9F66
                        text_style +: { font_size: 10 }
                    }
                    draw_bg +: {
                        color: #00000000
                        color_hover: #xEAD8B822
                        border_size: 0.0
                        border_radius: 6.0
                    }
                }
            }
        }
    }

    // W04 / M2 — DockRow prototype. Pulled to script_mod top level so the
    // `row_0..row_7 := DockRow {}` slots inside TaskDock's expanded body can
    // reference it. Defining `DockRow := View { ... }` *inside* TaskDock's
    // body created an instance child named `DockRow`, not a reusable
    // prototype, so the eight `row_N := DockRow {}` lookups crashed at live
    // eval with `variable DockRow not found in scope`. Mirrors the
    // `let RiskBadge = ...` pattern in `app/src/app/approvals.rs`.
    let DockRow = View {
        width: Fill
        height: Fit
        flow: Right
        spacing: 8
        align: Align{y: 0.5}
        padding: Inset{left: 4 top: 2 right: 4 bottom: 2}

        row_icon := Label {
            width: 18
            text: "🔧"
            draw_text.color: #xF6BE63
            draw_text.text_style.font_size: 12
        }
        row_name := Label {
            width: Fill
            text: ""
            draw_text.color: ai_cream
            draw_text.text_style.font_size: 11
        }
        row_status := Label {
            width: Fit
            text: ""
            draw_text.color: #x72E4FF
            draw_text.text_style.font_size: 10
        }
        row_detail := Label {
            width: Fit
            text: ""
            visible: false
            draw_text.color: #xCDBF9F88
            draw_text.text_style.font_size: 10
            margin: Inset{left: 6}
        }
    }

    // W04 / M2 — TaskDock under the chat composer. Reads `APP_STATE.tool_calls`
    // and `APP_STATE.tasks` on each draw; the OctosUiAgent drains
    // `tool/*` and `task/*` notifications into the store
    // (`app/src/backend/octos_ui.rs::fold_into_store`). The Rust impl is in
    // `app/src/app/task_dock.rs`; this DSL block declares the visual layout.
    let TaskDock = #(crate::app::task_dock::TaskDock::register_widget(vm)) {
        width: Fill
        height: Fit
        flow: Down
        margin: Inset{left: 92 right: 92 top: 4 bottom: 0}
        spacing: 4

        header_row := View {
            width: Fill
            height: Fit
            flow: Right
            align: Align{y: 0.5}
            spacing: 6

            chevron := Label {
                width: Fit
                height: Fit
                text: "▸"
                draw_text.color: #xF6BE63
                draw_text.text_style.font_size: 11
            }

            // Pill behind the chevron + label. Click anywhere toggles the
            // expanded body (Rust handles the action).
            header_pill := ButtonFlat {
                width: Fill
                height: 26
                align: Align{x: 0.0 y: 0.5}
                padding: Inset{left: 10 right: 10}
                text: "🔧 0 tools · 0 tasks · 0% running"
                draw_text +: {
                    color: ai_cream
                    text_style +: { font_size: 11 }
                }
                draw_bg +: {
                    color: #x0A2E26C8
                    color_hover: #x123E32DD
                    border_color: #x72E4FF44
                    border_size: 1.0
                    border_radius: 12.0
                }
            }
        }

        // Expanded-state body. Visibility flipped from Rust on toggle. The
        // outer `RubberView` smoothes the height transition on expand /
        // collapse — same trick aichat uses for the streaming-markdown
        // assistant body (`aichat:480`, smoothing 0.3).
        body := RubberView {
            width: Fill
            height: Fit
            smoothing: 0.3
            visible: false
            margin: Inset{top: 4}
            padding: Inset{left: 10 top: 8 right: 10 bottom: 8}
            spacing: 4
            show_bg: true
            draw_bg +: {
                color: #x062821CC
                radius: 10.0
            }

            row_0 := DockRow {}
            row_1 := DockRow {}
            row_2 := DockRow {}
            row_3 := DockRow {}
            row_4 := DockRow {}
            row_5 := DockRow {}
            row_6 := DockRow {}
            row_7 := DockRow {}

            overflow := Label {
                width: Fill
                height: Fit
                text: ""
                visible: false
                margin: Inset{top: 4}
                draw_text.color: #xCDBF9FAA
                draw_text.text_style.font_size: 10
            }
        }
    }

    // W07 / M3 — Studio / Slides / Sites producer screens (DSL inline,
    // Rust impl at `app/src/app/producers.rs`). Mirrors the SessionList
    // / TaskDock pattern. The chat pane in each triptych embeds the
    // local `ChatList` binding directly, satisfying W07's "the chat
    // thread inside each producer MUST be the same `ChatList` widget".

    let ProducerHeading = Label {
        width: Fill height: Fit margin: Inset{top: 0 bottom: 4 left: 2 right: 2}
        draw_text.color: #xCDBF9FA0 draw_text.text_style.font_size: 11
    }

    let GenerationCard = #(crate::app::producers::GenerationCardWidget::register_widget(vm)) {
        width: Fill height: Fit flow: Down spacing: 4 show_bg: true
        margin: Inset{top: 3 bottom: 3 left: 4 right: 4}
        padding: Inset{left: 10 top: 8 right: 10 bottom: 8}
        draw_bg +: { color: #x0A2A22DD radius: 10.0 }

        gen_header := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 6
            gen_kind_label := Label {
                width: Fit text: ""
                draw_text.color: #x72E4FF draw_text.text_style.font_size: 10
            }
            View { width: Fill height: 1 }
            gen_open_button := ButtonFlat {
                width: Fit height: 22 text: "Open"
                padding: Inset{left: 8 right: 8}
                draw_text +: {
                    color: #xF3E3C7
                    text_style +: { font_size: 10 }
                }
                draw_bg +: {
                    color: #x08251EC8 color_hover: #x123B31DD
                    border_color: #xEAD8B83A border_size: 1.0 border_radius: 8.0
                }
            }
        }
        gen_title_label := Label {
            width: Fill height: Fit text: ""
            draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 12
        }
    }

    // Shared body for the three producer screens. Used via `..ProducerBody{}`
    // spread in each `mod.widgets.{Studio,Slides,Sites}Screen` below.
    let ProducerBody = View {
        width: Fill height: Fill flow: Down spacing: 8

        producer_header := View {
            width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 6
            producer_title := Label {
                width: Fit text: ""
                draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 16
            }
            producer_subtitle := Label {
                width: Fit text: ""
                margin: Inset{left: 8}
                draw_text.color: #xCDBF9F88 draw_text.text_style.font_size: 11
            }
        }

        producer_body := View {
            width: Fill height: Fill flow: Right spacing: 12

            source_pane := View {
                width: 320 height: Fill flow: Down spacing: 6
                ProducerHeading { text: "Sources" }
                source_input := TextInput {
                    width: Fill height: 56 empty_text: "URL, pasted text, or PDF reference"
                    draw_bg +: {
                        color: #x06241DCC color_hover: #x0A2D24DD color_focus: #x0F362DEE
                        color_empty: #x06241DCC border_color: #x72E4FF44
                        border_size: 1.0 border_radius: 10.0
                    }
                    draw_text +: {
                        color: #xF3E3C7 color_empty: #xF3E3C766
                        text_style +: { font_size: 12 }
                    }
                }
                add_source_button := ButtonFlat {
                    width: Fill height: 32 text: "+ Add Source"
                    padding: Inset{left: 12 right: 12}
                    draw_text +: { color: #xF3E3C7 text_style +: { font_size: 11 } }
                    draw_bg +: {
                        color: #x08251EC8 color_hover: #x123B31DD
                        border_color: #xEAD8B83A border_size: 1.0 border_radius: 10.0
                    }
                }
                source_divider := SolidView {
                    width: Fill height: 1 margin: Inset{top: 4 bottom: 4}
                    draw_bg.color: #xEAD8B81C
                }
                source_list_heading := ProducerHeading { text: "Added" }
                source_list := PortalList {
                    width: Fill height: Fill flow: Down
                    drag_scrolling: true auto_tail: false
                    SourceRow := RoundedView {
                        width: Fill height: Fit
                        margin: Inset{top: 2 bottom: 2 left: 0 right: 0}
                        padding: Inset{left: 8 top: 5 right: 8 bottom: 5}
                        show_bg: true
                        draw_bg +: { color: #x06231CCC radius: 6.0 }
                        source_text_label := Label {
                            width: Fill text: ""
                            draw_text.color: #xCDBF9FCC
                            draw_text.text_style.font_size: 11
                        }
                    }
                }
                source_empty := Label {
                    width: Fill height: Fit
                    text: "No sources yet."
                    visible: true
                    margin: Inset{top: 6}
                    draw_text.color: #xCDBF9F77
                    draw_text.text_style.font_size: 11
                }
            }

            // Per W07 brief: reuse the W03 `ChatList` widget directly.
            // The chat thread is per-project — switching projects swaps
            // `APP_STATE.current_session` so this re-mounts cleanly.
            chat_pane := View {
                width: Fill height: Fill flow: Down spacing: 4
                ProducerHeading { text: "Chat" }
                producer_chat_list := ChatList {}
            }

            output_pane := View {
                width: 360 height: Fill flow: Down spacing: 6
                ProducerHeading { text: "Generations" }
                output_list := PortalList {
                    width: Fill height: Fill flow: Down
                    drag_scrolling: true auto_tail: false
                    GenRow := GenerationCard {}
                }
                output_empty := View {
                    width: Fill height: Fit flow: Down align: Align{x: 0.5 y: 0.5}
                    margin: Inset{top: 24} visible: true
                    Label {
                        text: "Generation history will appear here"
                        draw_text.color: #xF3E3C7 draw_text.text_style.font_size: 12
                    }
                    Label {
                        text: "(server producer tools land in the next slice)"
                        draw_text.color: #xCDBF9F77 draw_text.text_style.font_size: 10
                        margin: Inset{top: 4}
                    }
                }
            }
        }
    }

    // StudioScreen / SlidesScreen / SitesScreen templates removed —
    // unsupported in this build (their widgets remain in `producers.rs`).

    startup() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                show_caption_bar: false
                pass.clear_color: #00000000
                window.transparent: true
                // window.backdrop: WindowBackdrop.Blur — disabled until
                // platform bug fixed in macos_window.rs:532 (addSubview
                // positioned arg must be NSWindowBelow/-1 or NSWindowAbove/1,
                // not 0). See issues/aichat-liquid-glass-backdrop-platform-bug.md
                window.macos: MacosWindowConfig{chrome: MacosWindowChrome.Borderless resizable: true}
                window.inner_size: vec2(900, 700)
                window.title: " "
                body +: {
                    flow: Overlay
                    padding: 3
                    spacing: 0
                    draw_bg.color: #00000000

                    app_shell := GlassPanel {
                        width: Fill
                        height: Fill
                        new_batch: true
                        flow: Right
                        // Edge-to-edge: no frame inset so the A2App card fills
                        // the whole screen.
                        padding: Inset{left: 0 top: 0 right: 0 bottom: 0}
                        spacing: 0
                        draw_bg +: {
                            tint_color: #x0D4035
                            tint_alpha: 0.66
                            border_color: ai_cyan
                            border_alpha: 0.38
                            border_width: 1.0
                            corner_radius: 10.0
                            halo_color: ai_cyan
                            halo_strength: 0.0
                            halo_radius: 0.0
                            highlight_strength: 0.28
                            highlight_band_height: 58.0
                            chroma_strength: 0.0
                            noise_strength: 0.004
                        }

                    sidebar := GlassPanel {
                        width: 298
                        height: Fill
                        new_batch: true
                        flow: Down
                        padding: Inset{left: 14 top: 14 right: 14 bottom: 14}
                        spacing: 10
                        draw_bg +: {
                            tint_color: #x0A3A30
                            tint_alpha: 0.78
                            border_color: #xEAD8B8
                            border_alpha: 0.20
                            border_width: 0.0
                            corner_radius: 0.0
                            halo_strength: 0.0
                            halo_radius: 0.0
                            highlight_strength: 0.16
                            highlight_band_height: 54.0
                            chroma_strength: 0.0
                            noise_strength: 0.004
                        }

                        sidebar_header := View {
                            width: Fill
                            height: Fit
                            flow: Down
                            spacing: 8
                            margin: Inset{top: 4 bottom: 18}

                            View {
                                width: Fill
                                height: Fit
                                flow: Right
                                spacing: 10
                                align: Align{y: 0.5}

                                Label {
                                    text: "AI"
                                    draw_text.color: ai_cyan
                                    draw_text.text_style.font_size: 14
                                }

                                Label {
                                    text: "Octos"
                                    draw_text.color: ai_cream
                                    draw_text.text_style.font_size: 15
                                }
                            }

                            Label {
                                text: "Diagram workspace"
                                draw_text.color: ai_cream_dim
                                draw_text.text_style.font_size: 11
                            }
                        }

                        nav_new := ButtonFlat {
                            width: Fill
                            height: 38
                            text: "+  新对话"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 14 right: 12}
                            draw_text +: {
                                color: ai_cream
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #x0B6B67AA
                                color_hover: #x108E88CC
                                border_color: #x72E4FF66
                                border_size: 1.0
                                border_radius: 10.0
                            }
                        }

                        nav_search := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "⌕  搜索"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        nav_plugins := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "⌘  插件"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        nav_automation := ButtonFlat {
                            width: Fill
                            height: 30
                            text: ">  自动化"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        // W04 / M2 — Content nav button. Replaces the
                        // inactive `nav_project` placeholder per
                        // `04-IA-AND-NAVIGATION.md` § Top-level shell
                        // ("Content" sidebar item). Click dispatches
                        // through App::handle_actions to flip
                        // `APP_STATE.navigation` to `CurrentScreen::Content`.
                        nav_content := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "📚  内容"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xE4D4B6
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        // Coding / Studio / Slides / Sites navs removed —
                        // not supported in this build (user directive). The
                        // screens' widget modules stay registered for when
                        // the server-side tools land.

                        Label {
                            text: "对话"
                            margin: Inset{top: 28 bottom: 2 left: 0 right: 0}
                            draw_text.color: #xCDBF9FA0
                            draw_text.text_style.font_size: 12
                        }

                        // W04 — live session list. Empty until a server is
                        // connected; `App::handle_startup` calls
                        // `crate::app::sessions::hydrate_sessions` once the
                        // RestClient is ready. Click selects, x deletes.
                        session_list := SessionList {
                            width: Fill
                            height: Fill
                        }

                        settings_button := ButtonFlat {
                            width: Fill
                            height: 32
                            text: "*  设置"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xF3E3C7
                                text_style +: { font_size: 12 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }

                        // W08 — Sign-out link. On click `App::handle_actions`
                        // wipes the keychain entry for `(host, profile_id)`,
                        // clears the in-memory auth slice, and flips the
                        // login_overlay back on.
                        sign_out_button := ButtonFlat {
                            width: Fill
                            height: 28
                            text: "↪  退出登录"
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 4 right: 4}
                            draw_text +: {
                                color: #xCDBF9F88
                                text_style +: { font_size: 11 }
                            }
                            draw_bg +: {
                                color: #00000000
                                color_hover: #xEAD8B814
                                border_size: 0.0
                                border_radius: 8.0
                            }
                        }
                    }

                    SolidView {
                        width: 1
                        height: Fill
                        draw_bg.color: #xEAD8B81E
                    }

                    main_area := GlassPanel {
                        width: Fill
                        height: Fill
                        new_batch: true
                        flow: Down
                        // Edge-to-edge full-screen card: zero padding.
                        padding: Inset{left: 0 top: 0 right: 0 bottom: 0}
                        spacing: 0
                        draw_bg +: {
                            tint_color: #x0B3B31
                            tint_alpha: 0.70
                            border_color: #xEAD8B8
                            border_alpha: 0.16
                            border_width: 0.0
                            corner_radius: 0.0
                            halo_strength: 0.0
                            halo_radius: 0.0
                            highlight_strength: 0.16
                            highlight_band_height: 56.0
                            chroma_strength: 0.0
                            noise_strength: 0.004
                        }

                        top_bar := View {
                            width: Fill
                            height: 40
                            flow: Right
                            align: Align{y: 0.5}
                            // Minimalist full-screen A2App: no header chrome.
                            visible: false

                            // Phone: the sidebar auto-collapses after nav
                            // clicks on narrow windows; this brings it back.
                            nav_toggle := ButtonFlat {
                                width: 34
                                height: 30
                                text: "☰"
                                margin: Inset{right: 8}
                                align: Align{x: 0.5 y: 0.5}
                                draw_text +: {
                                    color: #xE4D4B6
                                    text_style +: { font_size: 14 }
                                }
                                draw_bg +: {
                                    color: #00000000
                                    color_hover: #xEAD8B814
                                    border_size: 0.0
                                    border_radius: 8.0
                                }
                            }

                            Label {
                                text: "Octos"
                                draw_text.color: ai_cream
                                draw_text.text_style.font_size: 14
                            }

                            // W04 follow-up #3 — connection state dot.
                            // Colour is updated from `App::update_connection_indicator`
                            // by re-evaluating the label's color: green = Live,
                            // amber = Reconnecting, red = Offline / Failed.
                            connection_dot := Label {
                                text: "●"
                                margin: Inset{left: 8 right: 4}
                                draw_text.color: #x6F8F6F
                                draw_text.text_style.font_size: 12
                            }
                            connection_state_label := Label {
                                text: ""
                                draw_text.color: ai_cream_dim
                                draw_text.text_style.font_size: 11
                            }

                            // Live context-window usage — updated every turn
                            // from `context/normalization` (App::update_context_indicator).
                            // Shows how full the model's context is, so the
                            // server-side compaction that keeps it bounded is
                            // visible rather than invisible.
                            context_chip := Label {
                                text: ""
                                margin: Inset{left: 10}
                                draw_text.color: #x8FB8A6
                                draw_text.text_style.font_size: 11
                            }

                            View { width: Fill height: 1 }

                            ToolbarGlass {
                                // Slimmed for phone viewports (was 286 with a
                                // "Profile" caption — clipped at 384pt).
                                width: 150

                                // Renamed from `backend_dropdown` per W02 §
                                // "Top bar contents" — same widget shape, but
                                // populated with the user's Octos profiles
                                // (W08 will swap in real labels). Stub label
                                // ships in M1 so the dropdown isn't empty.
                                backend_dropdown := DropDown {
                                    width: Fill
                                    height: 30
                                    popup_menu_position: PopupMenuPosition.BelowInput
                                    labels: ["(no profile)"]
                                    popup_menu: PopupMenuFlat{
                                        width: 170
                                        padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
                                        draw_bg +: {
                                            color: #x06231CF2
                                            border_color: #x72E4FF38
                                            border_size: 1.0
                                            border_radius: 12.0
                                        }
                                        menu_item: PopupMenuItem{
                                            height: 26
                                            padding: Inset{left: 18 right: 10 top: 0 bottom: 0}
                                            draw_text +: {
                                                color: ai_cream
                                                color_hover: #xFFF0D2
                                                color_active: ai_cream
                                                text_style +: { font_size: 11 }
                                            }
                                            draw_bg +: {
                                                color: #x00000000
                                                color_hover: #x123B31DD
                                                color_active: #xEAD8B82D
                                                border_color: #x00000000
                                                border_color_hover: #x72E4FF22
                                                border_color_active: #x72E4FF44
                                                border_size: 1.0
                                                border_radius: 6.0
                                                mark_color_active: ai_gold
                                            }
                                        }
                                    }
                                    draw_text +: {
                                        color: ai_cream
                                        text_style +: { font_size: 11 }
                                    }
                                    draw_bg +: {
                                        color: #x08251ED8
                                        color_hover: #x12382FEE
                                        border_color: #xEAD8B832
                                        border_size: 1.0
                                        border_radius: 10.0
                                        arrow_color: ai_cream
                                    }
                                }
                            }

                            glass_toolbar := ToolbarGlass {
                                width: 318
                                margin: Inset{left: 12}

                                ToolbarLabel {
                                    text: "Glass"
                                    width: 54
                                }

                                opacity_slider := GlassSlider {}

                                opacity_value := Label {
                                    width: 42
                                    text: "90%"
                                    margin: Inset{left: 4}
                                    draw_text.color: ai_cream_dim
                                    draw_text.text_style.font_size: 11
                                }
                            }
                        }

                        // W04 / M2 — `chat_screen` wrapper. Holds the chat
                        // thread + approvals + composer + task dock as one
                        // visibility unit so the sibling `content_screen`
                        // can swap in when `CurrentScreen::Content` is
                        // active. App::handle_actions toggles `set_visible`
                        // in lockstep (mirrors the W08 login_overlay
                        // pattern, app/src/main.rs:1433).
                        chat_screen := View {
                            width: Fill
                            height: Fill
                            // Down flow: card fills the space, composer docks at
                            // the bottom. A true Overlay float broke touch routing
                            // over a FULL-SCREEN card (the PortalList swallowed
                            // taps meant for the floating pill), so the composer
                            // docks below the card instead — it still auto-hides
                            // to the reveal pill, and docking avoids covering the
                            // card's bottom text.
                            flow: Down
                            spacing: 0

                        chat_shell := View {
                            width: Fill
                            height: Fill
                            flow: Overlay

                            empty_state := View {
                                width: Fill
                                height: Fill
                                flow: Down
                                align: Align{x: 0.5 y: 0.46}
                                spacing: 18

                                Label {
                                    text: "我们该做什么？"
                                    draw_text.color: #xF3E3C7
                                    draw_text.text_style.font_size: 27
                                }

                                Label {
                                    text: "输入自然语言，生成可交互的 Makepad diagram。"
                                    draw_text.color: #xCDBF9FAA
                                    draw_text.text_style.font_size: 12
                                }
                            }

                            chat_list := ChatList {}
                        }

                        // W05 — typed approval cards. The pane hides itself
                        // when `APP_STATE.approvals` is empty (see
                        // `app/src/app/approvals.rs::draw_walk`); when
                        // approvals are pending it pins above the composer.
                        approvals_pane := ApprovalsPane {}

                        // toast_row + octo_row live inside composer_row (bottom
                        // stack) so the thinking indicator and toasts sit just
                        // above the floating composer — not at the top of the
                        // Overlay flow.

                        composer_row := View {
                            width: Fill
                            height: Fit
                            flow: Down
                            align: Align{x: 0.5}

                            // Toast strip — one auto-dismissing pill for
                            // compaction / memory-saved / warning messages
                            // (App::sync_toasts drives it from APP_STATE.toasts).
                            toast_row := View {
                                width: Fill
                                height: Fit
                                visible: false
                                align: Align{x: 0.5}
                                toast_pill := RoundedView {
                                    width: Fit
                                    height: Fit
                                    margin: Inset{top: 2 bottom: 4}
                                    padding: Inset{left: 14 top: 8 right: 14 bottom: 8}
                                    show_bg: true
                                    draw_bg +: {
                                        color: #x0C3A2FF2
                                        radius: 10.0
                                    }
                                    toast_label := Label {
                                        width: Fit
                                        height: Fit
                                        text: ""
                                        draw_text.color: #xDCEAE0
                                        draw_text.text_style.font_size: 11
                                    }
                                }
                            }

                            // Swimming-octopus thinking indicator — visible only
                            // while a turn is streaming (`is_streaming`); sits
                            // directly above the composer.
                            octo_row := View {
                                width: Fill
                                height: Fit
                                visible: false
                                align: Align{x: 0.5}
                                octo := OctoThinking {}
                            }

                            // Collapsed state: a slim translucent pill that
                            // reveals the composer again (it auto-hides after a
                            // card renders). Only one of pill/composer is visible
                            // at a time; they stack at the bottom of this flow.
                            reveal_pill := PillButton {
                                text: "+"
                                width: 52
                                height: 30
                                visible: false
                                margin: Inset{bottom: 12}
                                draw_text +: {
                                    color: ai_cream
                                    text_style +: { font_size: 18 }
                                }
                                draw_bg +: {
                                    color: #x0B4035B0
                                    color_hover: #x123B31D0
                                    border_color: #x72E4FF44
                                    border_size: 1.0
                                    border_radius: 15.0
                                }
                            }

                            composer := GlassPanel {
                                // No min-width: a 620pt floor pushed the
                                // composer (and its Send button) off-screen
                                // on portrait phones (~384pt viewport).
                                width: Fill{max: 1040}
                                height: Fit
                                new_batch: true
                                flow: Down
                                margin: Inset{left: 12 right: 12}
                                padding: Inset{left: 14 top: 5 right: 12 bottom: 5}
                                spacing: 2
                                draw_bg +: {
                                    tint_color: #x0B4035
                                    // Floats over the card — keep it translucent
                                    // (liquid glass) so the card shows through.
                                    tint_alpha: 0.50
                                    border_color: ai_cyan
                                    border_alpha: 0.42
                                    border_width: 1.0
                                    corner_radius: 11.0
                                    halo_color: ai_cyan
                                    halo_strength: 0.05
                                    halo_radius: 3.0
                                    highlight_strength: 0.24
                                    highlight_band_height: 28.0
                                    chroma_strength: 0.0
                                    noise_strength: 0.003
                                }

                                input := TextInput {
                                    width: Fill
                                    height: 32
                                    // Soft keyboards: show a Send action key
                                    // (ImeAction::Send submits via the same
                                    // path as the ↑ button). Without this the
                                    // on-screen Enter did nothing visible.
                                    return_key_type: Send
                                    empty_text: "问任何事…"
                                    draw_bg +: {
                                        color: #00000000
                                        color_hover: #00000000
                                        color_focus: #00000000
                                        border_size: 0.0
                                        border_radius: 0.0
                                    }
                                    // Per-instance font override — TextInput bakes
                                    // `theme.font_regular` at DSL-expansion time, same
                                    // issue as Markdown/CodeView. Without this the
                                    // input box shows tofu for CJK and U+2192 arrows.
                                    draw_text +: {
                                        color: ai_cream
                                        color_empty: ai_cream_dim
                                        text_style: theme.font_regular{
                                            line_spacing: theme.font_wdgt_line_spacing
                                            font_size: 13
                                            font_family: FontFamily{
                                                latin := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                                                chinese := FontMember{res: crate_resource("self:resources/LXGWWenKaiMono-Regular.ttf") asc: 0.0 desc: 0.0}
                                                symbols := FontMember{res: crate_resource("self:resources/NotoSans-Regular.ttf") asc: 0.0 desc: 0.0}
                                                emoji := FontMember{res: crate_resource("self:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                                            }
                                        }
                                    }
                                }

                                composer_actions := View {
                                    width: Fill
                                    height: Fit
                                    flow: Right
                                    align: Align{y: 0.5}
                                    spacing: 6

                                    attach_button := IconButton { text: "+" width: 30 height: 30 }

                                    // @ mention, ⌘ tools and 默认权限 stubs
                                    // dropped: all are M1 placeholders and
                                    // the row must fit a 384pt phone
                                    // viewport.

                                    // Thinking + A2App toggles removed — this app
                                    // is now an always-on A2App card generator
                                    // (splash_mode is forced true at startup).

                                    View { width: Fill height: 1 }

                                    cancel_button := ButtonFlat {
                                        text: "Cancel"
                                        width: 64
                                        height: 30
                                        visible: false
                                        draw_text +: {
                                            color: #xF2F4F8
                                            text_style +: { font_size: 11 }
                                        }
                                        draw_bg +: {
                                            color: #x4B332FCC
                                            color_hover: #x64413ADD
                                            border_color: #xEAD8B818
                                            border_size: 1.0
                                            border_radius: 10.0
                                        }
                                    }

                                    clear_button := ButtonFlatIcon {
                                        width: 34
                                        height: 30
                                        icon_walk: Walk{ width: 19, height: 19 }
                                        draw_icon +: {
                                            color: #xB6C6BE
                                            svg: crate_resource("self:resources/icons/clear.svg")
                                        }
                                        draw_bg +: {
                                            color: #00000000
                                            color_hover: #xEAD8B814
                                            border_size: 0.0
                                            border_radius: 8.0
                                        }
                                    }

                                    send_button := SendButton {
                                        width: 30
                                        height: 30
                                    }
                                }
                            }
                        }

                        // W04 / M2 — TaskDock placed below the composer per
                        // 04-IA-AND-NAVIGATION.md § ChatScreen ASCII layout.
                        // Idle state collapses to zero height (`set_visible`
                        // off when both tool_calls and tasks are empty for the
                        // current session). Smoothing animation lifted from
                        // `aichat:480` (RubberView wrapping the assistant
                        // message body).
                        task_dock := TaskDock {}
                        }

                        // W04 / M2 — Content browser screen. Sibling to
                        // `chat_screen`; only one of the two is visible at
                        // a time. App::handle_actions toggles
                        // `set_visible` based on `APP_STATE.navigation`.
                        // Hidden by default — the boot path keeps Chat as
                        // the active screen.
                        content_screen := ContentBrowser {
                            visible: false
                        }

                        // Coding / Studio / Slides / Sites screens removed —
                        // unsupported in this build (user directive).

                        status_label := Label {
                            width: Fill
                            height: Fit
                            text: "Initializing..."
                            margin: Inset{left: 12 right: 12 top: 0 bottom: 0}
                            draw_text.text_style.font_size: 10
                            draw_text.color: #xE2D2B9AA
                            // Minimalist full-screen A2App: no footer chrome.
                            visible: false
                        }
                    }
                    }

                    // W08 — LoginScreen overlay. Lives at the body level
                    // (sibling to `app_shell`) so its hit-region covers
                    // everything when visible. App-side boot / login flow
                    // toggles `app_shell.visible` and `login_overlay.visible`
                    // in lockstep so only one of the two is interactive at a
                    // time. Default: hidden — `App::after_new_from_script`
                    // flips it on if no token is in the keychain. Resize
                    // grip stays after this in z-order so the user can
                    // resize the window even from Login.
                    login_overlay := LoginScreen {
                        visible: false
                    }

                    // W04 / M2 — File-viewer overlay (sibling to
                    // `app_shell` so it covers the whole window when a
                    // file is opened). Toggled by App::handle_actions on
                    // ContentAction::Open. Mirrors `login_overlay`.
                    viewer_overlay := ViewerOverlay {}

                    resize_grip := Vector{
                        width: 34
                        height: 34
                        margin: Inset{right: 18 bottom: 18}
                        align: Align{x: 1.0 y: 1.0}
                        viewbox: vec4(0 0 34 34)
                        Path{d: "M 18 28 L 28 18" fill: false stroke: #xEAD8B8AA stroke_width: 1.5 stroke_linecap: "round"}
                        Path{d: "M 12 28 L 28 12" fill: false stroke: #xF3E3C788 stroke_width: 1.2 stroke_linecap: "round"}
                        Path{d: "M 24 28 L 28 24" fill: false stroke: #x9F7E4BAA stroke_width: 1.5 stroke_linecap: "round"}
                    }
                }
            }
        }
    }
}

// Global chat state accessible to ChatList widget
pub static CHAT_DATA: std::sync::RwLock<ChatData> = std::sync::RwLock::new(ChatData {
    messages: Vec::new(),
    streaming_text: String::new(),
    thinking_text: String::new(),
    is_streaming: false,
    a2app_state: std::collections::BTreeMap::new(),
});

// Slider position range (NOT alpha — alpha is derived per-layer).
const DEFAULT_GLASS_OPACITY: f64 = 0.90;
const MIN_GLASS_OPACITY: f64 = 0.10;
const MAX_GLASS_OPACITY: f64 = 1.00;

#[derive(Debug, Clone, Copy, PartialEq)]
struct GlassOpacity {
    app: f32,
    sidebar: f32,
    main: f32,
    composer: f32,
}

// Map slider [0.10..1.00] to actual panel alpha. The earlier mapping only
// moved alpha slightly, so the "Glass" control felt inert on a transparent
// window. Keep layer ordering, but make the low/high ends visually obvious.
fn glass_opacity_values(slider: f64) -> GlassOpacity {
    let t = ((slider.clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY) - MIN_GLASS_OPACITY)
        / (MAX_GLASS_OPACITY - MIN_GLASS_OPACITY)) as f32;
    let shell = 0.28 + t * 0.64;
    GlassOpacity {
        app: shell,
        main: (shell + 0.05).min(0.99),
        sidebar: (shell + 0.08).min(0.99),
        composer: (shell + 0.11).min(0.99),
    }
}

fn should_start_window_drag(abs: DVec2, size: DVec2) -> bool {
    const RESIZE_EDGE_MARGIN: f64 = 10.0;
    const DRAG_STRIP_HEIGHT: f64 = 52.0;
    const RIGHT_TOOLBAR_WIDTH: f64 = 260.0;

    abs.y > RESIZE_EDGE_MARGIN
        && abs.y < DRAG_STRIP_HEIGHT
        && abs.x > RESIZE_EDGE_MARGIN
        && abs.x < size.x - RESIZE_EDGE_MARGIN
        && abs.x < size.x - RIGHT_TOOLBAR_WIDTH
}

// Diagram-fence safety scanner moved to `app/diagram_safety.rs` — same
// behaviour, just lifted out of main.rs for readability. The functions are
// re-exported below so the streaming pipeline (chat list redraw +
// `handle_event` on `TurnComplete`) doesn't need to qualify the path.
// `assistant_message_is_safe_for_history` is only referenced from the
// regression tests in `mod tests`; allow `unused` so a non-test build
// doesn't warn.
#[allow(unused_imports)]
use crate::app::diagram_safety::{
    assistant_message_is_safe_for_history, assistant_message_is_safe_to_store,
    unwrap_outer_markdown_fence,
};
// W04 — `SessionList` widget + REST hydrate plumbing. The widget type is
// referenced from the `let SessionList = …` register block in script_mod
// above via the fully-qualified `crate::app::sessions::SessionList` path,
// so no `use` for it here. The `SessionListAction` variants are folded in
// `App::handle_actions`.
use crate::app::sessions::{self as sessions_mod, SessionListAction, APP_STATE};
// W04 / M2 — content browser + viewers actions. Action variants land via
// `Cx::post_action` and are folded in `App::handle_actions`. State globals
// (`CONTENT_STATE`, `VIEWER_STATE`) mirror the `APP_STATE` pattern.
use crate::app::content_browser::{
    self as content_mod, ContentAction, ContentFilter, CONTENT_STATE,
};
use crate::app::viewers::{
    self as viewers_mod, OpenViewer, ViewerAction, VIEWER_STATE,
};
use octos_app_store::navigation::{CurrentScreen, NavigationEvent};
use octos_app_transport::rest::MyContentQuery;

/// Map a `recorded_decision` string from a server `-32011 APPROVAL_NOT_PENDING`
/// error payload back to an `ApprovalDecision`. The wire form is
/// `serde_json` snake_case (`"approve"` / `"deny"`); see octos-core
/// `ui_protocol.rs:564-569`.
fn parse_recorded_decision(s: &str) -> Option<octos_core::ui_protocol::ApprovalDecision> {
    use octos_core::ui_protocol::ApprovalDecision;
    match s {
        "approve" => Some(ApprovalDecision::Approve),
        "deny" => Some(ApprovalDecision::Deny),
        _ => None,
    }
}

// (W02 strip) — `CHAT_SAVE_PATH` (`aichat_history.json`),
// `stateless_history_messages` and the `SavedHistory` / `SavedMessage`
// SerJson types lived here. They're gone: Octos sessions are stateful
// server-side, so we don't replay history into a stateless backend, and the
// flat-file JSON cache is replaced by per-session SQLite + REST hydrate
// (W04). See `01-ARCHITECTURE.md` § "Persistence".

#[derive(Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
}

#[derive(Script, ScriptHook, Widget)]
pub struct MermaidSvgView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_svg: DrawSvg,
    #[live]
    draw_text: DrawText,
    #[live]
    draw_flow_dot: DrawColor,
    #[rust]
    doc: SvgDocument,
    #[rust]
    content_w: f64,
    #[rust]
    content_h: f64,
    #[rust]
    last_src_hash: u64,
    #[rust]
    pending_src_hash: u64,
    #[rust]
    cached_text_cmds: Vec<SvgTextCmd>,
    #[rust]
    cached_edges: Vec<SvgEdge>,
    #[rust(1.0f64)]
    zoom: f64,
    #[rust]
    pan: DVec2,
    #[rust]
    drag_start_abs: Option<DVec2>,
    #[rust]
    drag_start_pan: DVec2,
    #[rust]
    last_rect: Rect,
    #[rust]
    anim_t: f32,
    #[rust]
    next_frame: NextFrame,
}

impl MermaidSvgView {
    pub fn set_svg_str(&mut self, cx: &mut Cx, svg: &str) {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        svg.hash(&mut hasher);
        let hash = hasher.finish();
        if hash == self.last_src_hash && !self.doc.root.is_empty() {
            return;
        }

        self.last_src_hash = hash;
        self.doc = parse_svg(svg);
        self.cached_text_cmds = collect_text_cmds(&self.doc);
        self.cached_edges = collect_edges(&self.doc);
        self.draw_svg.cache_valid = false;
        self.draw_svg.set_doc_bounds(&self.doc);
        if let Some(vb) = self.doc.viewbox.as_ref() {
            self.draw_svg.content_bounds = (vb.x, vb.y, vb.x + vb.width, vb.y + vb.height);
            self.content_w = vb.width as f64;
            self.content_h = vb.height as f64;
            self.draw_svg.content_size = dvec2(self.content_w, self.content_h);
        }
        self.redraw(cx);
    }

    pub fn set_mermaid_src(&mut self, cx: &mut Cx, src: &str) {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let cleaned: String = src.chars().filter(|c| *c != '▋').collect();
        let trimmed = cleaned.trim();
        if trimmed.is_empty() || trimmed.len() < 8 {
            return;
        }

        let mut hasher = DefaultHasher::new();
        trimmed.hash(&mut hasher);
        let hash = hasher.finish();
        if hash == self.last_src_hash && !self.doc.root.is_empty() {
            return;
        }

        // Streaming debounce: render only when the same source arrives twice
        // in a row. During active token streaming the body changes every
        // frame; after a pause or close it stabilizes and renders once.
        if hash != self.pending_src_hash {
            self.pending_src_hash = hash;
            return;
        }

        match streaming_markdown_kit::render_mermaid_to_svg(trimmed) {
            Ok(svg) => {
                self.set_svg_str(cx, &svg);
                self.last_src_hash = hash;
            }
            Err(err) => {
                log!("mermaid render error: {:?}", err);
            }
        }
    }
}

impl Widget for MermaidSvgView {
    fn set_text(&mut self, cx: &mut Cx, v: &str) {
        self.set_mermaid_src(cx, v);
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if self.next_frame.is_event(event).is_some() {
            self.anim_t = (self.anim_t + 0.003).rem_euclid(1.0);
            self.next_frame = cx.new_next_frame();
            self.redraw(cx);
        }

        match event.hits_with_capture_overload(cx, self.draw_svg.area(), true) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if fe.tap_count >= 2 {
                    self.zoom = 1.0;
                    self.pan = DVec2::default();
                    self.drag_start_abs = None;
                    self.redraw(cx);
                } else {
                    self.drag_start_abs = Some(fe.abs);
                    self.drag_start_pan = self.pan;
                    cx.set_cursor(MouseCursor::Grabbing);
                }
            }
            Hit::FingerMove(fe) => {
                if let Some(start) = self.drag_start_abs {
                    self.pan = self.drag_start_pan + (fe.abs - start);
                    self.redraw(cx);
                }
            }
            Hit::FingerUp(_) => {
                if self.drag_start_abs.is_some() {
                    self.drag_start_abs = None;
                    cx.set_cursor(MouseCursor::Grab);
                }
            }
            Hit::FingerHoverIn(_) => cx.set_cursor(MouseCursor::Grab),
            Hit::FingerScroll(fs) => {
                if !fs.modifiers.is_primary() {
                    return;
                }
                let dy = if fs.scroll.y.abs() > f64::EPSILON {
                    fs.scroll.y
                } else {
                    fs.scroll.x
                };
                let factor = (1.0 - dy * 0.005).clamp(0.5, 2.0);
                let old_zoom = self.zoom.max(0.01);
                let new_zoom = (old_zoom * factor).clamp(0.2, 8.0);
                let local = fs.abs - self.last_rect.pos - self.pan;
                let content_local = local / old_zoom;
                self.pan = fs.abs - self.last_rect.pos - content_local * new_zoom;
                self.zoom = new_zoom;
                self.redraw(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if self.doc.root.is_empty() {
            return DrawStep::done();
        }
        let sw = self.draw_svg.content_size.x;
        let sh = self.draw_svg.content_size.y;
        if sw <= 0.0 || sh <= 0.0 {
            return DrawStep::done();
        }
        let walk = Walk {
            abs_pos: walk.abs_pos,
            margin: walk.margin,
            width: match walk.width {
                Size::Fit { .. } => Size::Fixed(sw),
                other => other,
            },
            height: match walk.height {
                Size::Fit { .. } => Size::Fixed(sh),
                other => other,
            },
            metrics: walk.metrics,
        };
        let rect = cx.walk_turtle(walk);
        self.last_rect = rect;

        let zoom = if self.zoom > 0.01 { self.zoom } else { 1.0 };
        let effective_rect = Rect {
            pos: rect.pos + self.pan,
            size: rect.size * zoom,
        };

        self.draw_svg.svg_doc = Some(std::mem::take(&mut self.doc));
        self.draw_svg.has_animations = false;
        self.draw_svg.render_to_rect(cx, &effective_rect, 0.0);
        self.doc = self.draw_svg.svg_doc.take().unwrap_or_default();

        let text_cmds = std::mem::take(&mut self.cached_text_cmds);
        self.render_text_cmds(cx, &effective_rect, &text_cmds);
        self.cached_text_cmds = text_cmds;

        let edges = std::mem::take(&mut self.cached_edges);
        self.render_flow_dots(cx, &effective_rect, &edges);
        let has_edges = !edges.is_empty();
        self.cached_edges = edges;

        if has_edges {
            self.next_frame = cx.new_next_frame();
        }
        DrawStep::done()
    }
}

impl MermaidSvgView {
    fn render_text_cmds(&mut self, cx: &mut Cx2d, rect: &Rect, cmds: &[SvgTextCmd]) {
        if cmds.is_empty() {
            return;
        }
        let (min_x, min_y, max_x, max_y) = self.draw_svg.content_bounds;
        let content_w = (max_x - min_x) as f64;
        let content_h = (max_y - min_y) as f64;
        if content_w <= 0.0 || content_h <= 0.0 {
            return;
        }
        let scale = (rect.size.x / content_w).min(rect.size.y / content_h);
        let render_w = content_w * scale;
        let render_h = content_h * scale;
        let origin_x = rect.pos.x + (rect.size.x - render_w) * 0.5;
        let origin_y = rect.pos.y + (rect.size.y - render_h) * 0.5;
        const PX_TO_PT: f64 = 0.75;

        for cmd in cmds {
            if cmd.text.trim().is_empty() {
                continue;
            }
            let world_font_size = (cmd.font_size as f64 * scale * PX_TO_PT).max(1.0);
            self.draw_text.text_style.font_size = world_font_size as f32;
            self.draw_text.color = vec4(
                cmd.color.0,
                cmd.color.1,
                cmd.color.2,
                cmd.color.3.max(0.0),
            );

            let lines: Vec<&str> = cmd.text.split('\n').collect();
            let line_step_screen = world_font_size * 1.2;
            let base_cy = origin_y + (cmd.y as f64 - min_y as f64) * scale;
            let base_cx_screen = origin_x + (cmd.x as f64 - min_x as f64) * scale;

            for (line_index, line) in lines.iter().enumerate() {
                if line.is_empty() {
                    continue;
                }
                let estimated_width: f64 = line
                    .chars()
                    .map(|ch| {
                        let advance = if (ch as u32) >= 0x2E80 { 1.0 } else { 0.55 };
                        advance * world_font_size
                    })
                    .sum();
                let anchor_shift = match cmd.text_anchor {
                    SvgTextAnchor::Start => 0.0,
                    SvgTextAnchor::Middle => -0.5,
                    SvgTextAnchor::End => -1.0,
                } * estimated_width;

                let px = base_cx_screen + anchor_shift;
                let cy = base_cy + line_step_screen * line_index as f64;
                let py = cy - world_font_size * 0.7;
                self.draw_text.draw_abs(cx, dvec2(px, py), line);
            }
        }
    }

    fn render_flow_dots(&mut self, cx: &mut Cx2d, rect: &Rect, edges: &[SvgEdge]) {
        if edges.is_empty() {
            return;
        }
        let (min_x, min_y, max_x, max_y) = self.draw_svg.content_bounds;
        let content_w = (max_x - min_x) as f64;
        let content_h = (max_y - min_y) as f64;
        if content_w <= 0.0 || content_h <= 0.0 {
            return;
        }
        let scale = (rect.size.x / content_w).min(rect.size.y / content_h);
        let render_w = content_w * scale;
        let render_h = content_h * scale;
        let origin_x = rect.pos.x + (rect.size.x - render_w) * 0.5;
        let origin_y = rect.pos.y + (rect.size.y - render_h) * 0.5;
        let dot_size = 10.0_f64;
        let pulse =
            0.55 + 0.45 * (self.anim_t * std::f32::consts::TAU * 1.5).sin().abs();

        for (edge_index, edge) in edges.iter().enumerate() {
            if edge.points.len() < 2 {
                continue;
            }
            let phase = (self.anim_t + edge_index as f32 * 0.17).rem_euclid(1.0);
            let max_index = edge.points.len() - 1;
            let float_index = phase * max_index as f32;
            let point_index = float_index as usize;
            let next_index = (point_index + 1).min(max_index);
            let frac = float_index - point_index as f32;
            let p0 = edge.points[point_index];
            let p1 = edge.points[next_index];
            let wx = p0.0 + (p1.0 - p0.0) * frac;
            let wy = p0.1 + (p1.1 - p0.1) * frac;

            let sx = origin_x + (wx as f64 - min_x as f64) * scale;
            let sy = origin_y + (wy as f64 - min_y as f64) * scale;

            self.draw_flow_dot.color = vec4(
                edge.color.0,
                edge.color.1,
                edge.color.2,
                edge.color.3 * pulse,
            );
            self.draw_flow_dot.draw_abs(
                cx,
                Rect {
                    pos: dvec2(sx - dot_size * 0.5, sy - dot_size * 0.5),
                    size: dvec2(dot_size, dot_size),
                },
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
}

pub struct ChatData {
    pub messages: Vec<ChatMessage>,
    pub streaming_text: String,
    pub thinking_text: String,
    pub is_streaming: bool,
    /// Per-card A2App/Splash state: card (message index) → `CardState`. Each
    /// rendered card owns an isolated map so independent cards never share
    /// state; `{{state.<key>}}` substitutes that card's value. Mutated by
    /// `agent.notify` events tagged with the card's id (see `tag_notify_calls`).
    pub a2app_state: std::collections::BTreeMap<usize, CardState>,
}

impl ChatData {
    /// TODO: W04 — replace this no-op with a SQLite per-session cache write.
    /// aichat's flat `aichat_history.json` is gone (see
    /// `01-ARCHITECTURE.md` § "Persistence" — REST snapshot is the source of
    /// truth, the local cache is just a startup-warmer). Calls keep working
    /// so the streaming pipeline doesn't have to special-case anything.
    pub fn save_to_disk(&self) {
        // intentionally empty
    }

    /// TODO: W04 — hydrate from the per-session SQLite cache + REST
    /// snapshot. M1 returns an empty Vec so the empty state shows on boot.
    pub fn load_from_disk() -> Vec<ChatMessage> {
        Vec::new()
    }
}

// ChatList widget wrapping PortalList for chat message display.
#[derive(Script, ScriptHook, Widget)]
pub struct ChatList {
    #[deref]
    view: View,
    #[rust]
    animating_msg: Option<usize>,
}

impl Widget for ChatList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let data = CHAT_DATA.read().unwrap();

        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                let msg_count = data.messages.len();
                let items_len = msg_count + data.is_streaming as usize;
                list.set_item_range(cx, 0, items_len);

                while let Some(item_id) = list.next_visible_item(cx) {
                    if data.is_streaming && item_id == msg_count {
                        let just_started = self.animating_msg != Some(item_id);
                        if just_started {
                            self.animating_msg = Some(item_id);
                        }

                        let (item_widget, _) = list.item_with_existed(cx, item_id, id!(Assistant));
                        // Copy/share icons only appear once the answer is
                        // complete — hide them on the in-flight streaming item.
                        item_widget
                            .button(cx, ids!(copy_button))
                            .set_visible(cx, false);
                        item_widget
                            .button(cx, ids!(share_button))
                            .set_visible(cx, false);
                        let streaming_body;
                        // Reasoning/thinking is intentionally NOT surfaced in the
                        // chat bubble (user preference) — the swimming-octopus
                        // indicator conveys "working". Show only a minimal
                        // placeholder until the answer's first token arrives.
                        let text: &str = if data.streaming_text.is_empty() {
                            "…"
                        } else {
                            let opts = SanitizeOptions {
                                trim_unclosed_fence: false,
                                ..SanitizeOptions::default()
                            };
                            // Remend keeps fenced blocks, tables and math
                            // self-consistent mid-stream so the Markdown
                            // widget doesn't re-layout a half-closed block
                            // on every token. An open `runsplash` fence is
                            // deferred first — see `defer_unclosed_runsplash`.
                            let deferred = defer_unclosed_runsplash(&data.streaming_text);
                            streaming_body = streaming_display_with_latex_autowrap_remend(
                                &deferred,
                                opts,
                            );
                            &streaming_body
                        };
                        let mut markdown = item_widget.markdown(cx, ids!(selectable));
                        // Unwrap outer ```markdown wrapper in streaming
                        // content: some LLMs emit the wrapper as the very
                        // first tokens, so we'd otherwise render a growing
                        // code block for the whole stream.
                        let unwrapped_stream = unwrap_outer_markdown_fence(text);
                        let empty_state = CardState::new();
                        let card_state = data.a2app_state.get(&item_id).unwrap_or(&empty_state);
                        let resolved_stream =
                            resolve_a2app_card(unwrapped_stream, item_id, card_state);
                        markdown.set_text(cx, &resolved_stream);
                        if just_started {
                            markdown.reset_all_streaming_animations();
                        } else {
                            markdown.start_streaming_animation();
                        }
                        item_widget.draw_all_unscoped(cx);
                        continue;
                    }

                    if let Some(msg) = data.messages.get(item_id) {
                        // Full-screen splash app: don't echo the user's prompt —
                        // only the generated card is shown. Collapse the user
                        // item to zero height instead of rendering the bubble.
                        if matches!(msg.role, ChatRole::User) {
                            let item_widget = list.item(cx, item_id, id!(User));
                            item_widget.set_visible(cx, false);
                            item_widget.draw_all_unscoped(cx);
                            continue;
                        }
                        let is_animating = self.animating_msg == Some(item_id);
                        let template = match msg.role {
                            ChatRole::User => id!(User),
                            ChatRole::Assistant => id!(Assistant),
                        };
                        let item_widget = list.item(cx, item_id, template);
                        // Completed message — show the copy/share icons (PortalList
                        // pools items; this one may have been the hidden streaming
                        // item last frame). But NOT on an A2App card: copy/share act
                        // on the raw message text, which for a card is runsplash DSL,
                        // so the affordance is meaningless — hide both. User messages
                        // have neither button, so these are no-ops there.
                        let is_splash_card = msg.text.contains("```runsplash");
                        item_widget
                            .button(cx, ids!(copy_button))
                            .set_visible(cx, !is_splash_card);
                        item_widget
                            .button(cx, ids!(share_button))
                            .set_visible(cx, !is_splash_card);
                        let mut markdown = item_widget.markdown(cx, ids!(selectable));
                        // wrap_bare_latex wraps `\cmd{…}` with `$…$` so
                        // MathView can render them.
                        let unwrapped = unwrap_outer_markdown_fence(&msg.text);
                        let rendered = wrap_bare_latex(unwrapped);
                        let empty_state = CardState::new();
                        let card_state = data.a2app_state.get(&item_id).unwrap_or(&empty_state);
                        let rendered = resolve_a2app_card(&rendered, item_id, card_state);
                        markdown.set_text(cx, &rendered);
                        if is_animating {
                            markdown.stop_streaming_animation();
                        }
                        item_widget.draw_all_unscoped(cx);
                        if is_animating && markdown.is_streaming_animation_done() {
                            self.animating_msg = None;
                        }
                    }
                }
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        if let Event::Actions(actions) = event {
            let list = self.view.portal_list(cx, ids!(list));
            if list.any_items_with_actions(actions) {
                for (item_id, item) in list.items_with_actions(actions) {
                    let copy_btn = item.button(cx, ids!(copy_button));
                    if copy_btn.clicked(actions) {
                        let data = CHAT_DATA.read().unwrap();
                        if let Some(msg) = data.messages.get(item_id) {
                            cx.copy_to_clipboard(&msg.text);
                        }
                    }
                    // Share opens the OS share sheet (Android ACTION_SEND).
                    let share_btn = item.button(cx, ids!(share_button));
                    if share_btn.clicked(actions) {
                        let data = CHAT_DATA.read().unwrap();
                        if let Some(msg) = data.messages.get(item_id) {
                            cx.share_text(&msg.text);
                        }
                    }
                }
            }
        }
    }
}

// (W02 strip) — aichat's `BackendType` enum + `ALL_BACKENDS` constant + the
// inline `BackendType::system_prompt` (which baked the entire splash.md and
// diagram-kit JSON manual into the binary) lived here. They're gone: Octos
// serves all LLMs server-side and supplies system prompts per profile, so
// the client doesn't pick a backend or carry a prompt. See
// `05-AICHAT-REUSE-MAP.md` "Stuff we drop or replace" and
// `OCTOS_PLACEHOLDER_SYSTEM_PROMPT` near the top of this file. The original
// block lived at `aichat/examples/aichat/src/main.rs:1883–2072`.

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    /// Auto-dismiss timer for the toast strip (compaction / memory-saved /
    /// warnings). Empty when no toast is showing.
    #[rust]
    toast_timer: Timer,
    /// ~10 Hz repaint driver while a turn streams. Deltas only accumulate
    /// text + set `stream_dirty`; this interval turns them into redraws so a
    /// fast token stream doesn't re-parse/redraw the thread per token.
    #[rust]
    stream_tick: Timer,
    /// Set by delta handlers; cleared when the tick repaints.
    #[rust]
    stream_dirty: bool,
    /// "A2App" composer toggle: when on, the next message is wrapped with the
    /// Splash UI-generation prompt so the LLM returns a `runsplash` block that
    /// renders as live UI.
    #[rust]
    splash_mode: bool,
    /// Whether the Splash manual has already been sent into the current
    /// session. octos sessions are stateful server-side, so the ~85KB manual
    /// is primed once (first A2App message); later A2App messages send only a
    /// short instruction, avoiding re-sending it every turn. Reset on new chat.
    #[rust]
    splash_primed: bool,
    /// Whether the floating composer is expanded. It auto-collapses to the
    /// reveal pill after a card renders (full-screen viewing), and expands
    /// again when the pill is tapped. Initialized true in `handle_startup`.
    #[rust]
    composer_shown: bool,
    /// Single OctosUiAgent instance — replaces aichat's `Box<dyn Agent>`
    /// dynamic dispatch over LLM backends. Lazily constructed on first use.
    #[rust]
    agent: Option<Box<dyn Agent>>,
    #[rust]
    session_id: Option<SessionId>,
    #[rust]
    current_prompt: Option<PromptId>,
    /// Currently-selected Octos profile id (X-Profile-Id on the wire).
    /// `None` until W08 hydrates the profile list. Used by `update_status`.
    #[rust]
    current_profile: Option<ProfileId>,
    /// `(profile_id, display_label)` pairs for the top-bar dropdown.
    /// Empty in M1 — W08 calls `set_labels` once `/api/my/profile` lands.
    #[rust]
    available_profiles: Vec<(ProfileId, String)>,

    // ---- W08 — login flow state -------------------------------------------
    //
    // These are flat instead of an enum because the LoginScreen DSL keeps
    // the three step containers and toggles their `visible` flag, mirroring
    // the four-state machine in `workstreams/W08-auth-tenancy.md`
    // § "LoginScreen flow" (`Idle` / `SendingCode` / `AwaitingCode` /
    // `Verifying`). Verbose enum mapping isn't worth the indirection here.

    /// Once `Continue` (Step 1) succeeds we cache the parsed URL + profile
    /// id here so the email / verify steps can build a `RestClient` without
    /// re-reading `~/.config/octos-app/server.json`.
    #[rust]
    login_server_url: Option<url::Url>,
    /// Mirror of `ProfileId` from server config; threaded into the keychain
    /// service-name on a successful verify.
    #[rust]
    login_profile_id: Option<ProfileId>,
    /// Stashed across the Step 2 → Step 3 transition so `Verify` can resend
    /// the same email the OTP was issued against.
    #[rust]
    login_pending_email: Option<String>,

    /// W05 — handle exposed by `OctosUiAgent::approval_handle`, captured at
    /// agent-construction time so `App::handle_actions` can issue
    /// `approval/respond` without downcasting `Box<dyn Agent>`.
    /// Cheap-clone (`Sender<OutboundCommand>` + `tokio::runtime::Handle`).
    #[rust]
    approval_handle: Option<crate::backend::octos_ui::ApprovalHandle>,
    /// One-shot `task/output/read` handle for the coding task drill-down.
    #[rust]
    task_output_handle: Option<crate::backend::octos_ui::TaskOutputHandle>,
}

impl App {
    /// Construct an `OctosUiAgent` from the current process environment.
    /// W08 will plumb the bearer + profile through `octos-app-store::auth`
    /// and the keychain; for now we read placeholders so the binary boots
    /// without a server. Returns the boxed `Agent` so `App::agent` can stay
    /// `Option<Box<dyn Agent>>` and the streaming pipeline keeps working.
    ///
    /// Replaces aichat's per-backend `create_agent` match arm.
    /// Returns the boxed agent + the W05 approval handle (captured before
    /// the box hides the concrete type).
    fn create_octos_agent(
        transport_config: TransportConfig,
    ) -> (
        Box<dyn Agent>,
        crate::backend::octos_ui::ApprovalHandle,
        crate::backend::octos_ui::TaskOutputHandle,
    ) {
        let agent = OctosUiAgent::new(transport_config);
        let approval_handle = agent.approval_handle();
        let task_output_handle = agent.task_output_handle();
        (Box::new(agent) as Box<dyn Agent>, approval_handle, task_output_handle)
    }

    /// (Re)build the REST client + `OctosUiAgent` from the on-disk
    /// config/token state. Runs at boot and again after a successful login,
    /// so the WS transport picks up a fresh bearer without an app restart
    /// (the replaced agent drops its runtime + socket).
    ///
    /// W04 — the REST session hydrate fires before the agent steals the
    /// config. Empty bearer means we expect a 401; the failure path is
    /// silent in M1. W04 follow-up #5 — `/api/version` probe runs
    /// off-thread so we don't stall the caller.
    fn connect_transport(&mut self, cx: &mut Cx) {
        let transport_config = Self::placeholder_transport_config();
        log::info!(
            "connect transport: base_url={} profile_id={}",
            transport_config.base_url, transport_config.profile_id.0
        );
        // M12 D-5 — `GET /api/sessions` is retired server-side; the sidebar
        // hydrates over the WS (`session/list`) once `session/open` lands
        // (see `OctosUiAgent`'s `CapabilityNegotiated` arm). Only the public
        // version probe stays on REST.
        Self::probe_version(Self::build_rest_client(&transport_config));
        // Reflect the signed-in identity in the top bar: the Profile pill
        // previously shipped its "(no profile)" stub forever.
        let pid_str = transport_config.profile_id.0.clone();
        if !pid_str.is_empty() {
            self.available_profiles =
                vec![(ProfileId::from(pid_str.clone()), pid_str.clone())];
            self.current_profile = Some(ProfileId::from(pid_str.clone()));
            let dd = self.ui.drop_down(cx, ids!(backend_dropdown));
            dd.set_labels(cx, vec![pid_str]);
            dd.set_selected_item(cx, 0);
        }
        self.update_status(cx);
        let (agent, approval_handle, task_output_handle) =
            Self::create_octos_agent(transport_config);
        self.agent = Some(agent);
        self.approval_handle = Some(approval_handle);
        self.task_output_handle = Some(task_output_handle);
    }

    /// Build a `RestClient` from a `TransportConfig`. Used by W04 to hydrate
    /// the session list and to issue `DELETE /api/sessions/{id}`. Cheap —
    /// `reqwest::Client::new()` is `Arc`-shaped internally.
    fn build_rest_client(cfg: &TransportConfig) -> octos_app_transport::rest::RestClient {
        octos_app_transport::rest::RestClient::new(
            reqwest::Client::new(),
            cfg.base_url.clone(),
            cfg.bearer.clone(),
            cfg.profile_id.clone(),
        )
    }

    /// W04 follow-up #5 — fire `GET /api/version` once at boot. Logs the
    /// version + service, warns if the version doesn't start with `0.` /
    /// `1.` (so a mis-pointed server surfaces in the logs without
    /// blocking the boot path), and warns if `service != "octos"`.
    /// Off-thread; failures are silent (the live smoke can hit servers
    /// that don't serve `/api/version` yet).
    fn probe_version(client: octos_app_transport::rest::RestClient) {
        let _ = std::thread::Builder::new()
            .name("octos-version-probe".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        log::warn!("version probe: spawn tokio runtime: {e}");
                        return;
                    }
                };
                match rt.block_on(async { client.version_probe().await }) {
                    Ok(probe) => {
                        let version = probe.version_string();
                        let service = probe.service().map(str::to_owned);
                        log::info!(
                            "version probe: version={} service={}",
                            version.as_deref().unwrap_or("<unknown>"),
                            service.as_deref().unwrap_or("<unknown>"),
                        );
                        if let Some(v) = version.as_deref() {
                            if !v.starts_with("0.") && !v.starts_with("1.") {
                                log::warn!(
                                    "version probe: server reported {v}; expected 0.x or 1.x"
                                );
                            }
                        }
                        if let Some(s) = service.as_deref() {
                            if s != "octos" {
                                log::warn!(
                                    "version probe: service={s}; expected \"octos\" — wrong server?"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("version probe failed: {e}");
                    }
                }
            });
    }

    /// Build a `TransportConfig` for the boot REST hydrate + the WS agent.
    ///
    /// Resolution precedence (matches `boot_is_authed`):
    ///
    /// 1. **`~/.config/octos-app/server.json`** if present — `server_url`
    ///    becomes the REST `base_url`, `profile_id` is the `X-Profile-Id`.
    /// 2. **Bearer**: `OCTOS_APP_TOKEN` env var first (dev shortcut), else
    ///    `keychain::load_token(host, profile_id)` from the OS keychain.
    ///    Empty bearer is fine — REST will respond 401 and the failure path
    ///    is silent in M1.
    /// 3. **Fallback** (no server.json): the legacy `OCTOS_BASE_URL` /
    ///    `OCTOS_BEARER` / `OCTOS_PROFILE_ID` env vars + `https://localhost:8080`
    ///    so headless CI / `cargo run` without any config still boots.
    ///
    /// Renaming this away from the W01-era `placeholder_transport_config` is
    /// deferred — call sites also live in `handle_actions` and a rename
    /// would balloon the diff. The doc comment carries the new semantics.
    fn placeholder_transport_config() -> TransportConfig {
        // 1. server.json — happy path on a configured machine.
        if let Some(cfg) = crate::app::login::load_server_config() {
            if let Ok(base_url) = url::Url::parse(&cfg.server_url) {
                let profile_id = TransportProfileId::new(cfg.profile_id.clone());
                let bearer = Self::resolve_bearer(&base_url, &cfg.profile_id);
                return TransportConfig {
                    base_url,
                    bearer,
                    profile_id,
                    cursor: None,
                    requested_capabilities: Capabilities::requested(),
                    workspace_cwd: Self::current_workspace_cwd(),
                    stdio: Self::stdio_spawn(),
                };
            } else {
                log::warn!(
                    "server.json server_url failed to parse; falling back to OCTOS_BASE_URL env"
                );
            }
        }

        // 2. Env-only fallback (no server.json yet).
        let base_url = std::env::var("OCTOS_BASE_URL")
            .ok()
            .and_then(|s| url::Url::parse(&s).ok())
            .unwrap_or_else(|| {
                url::Url::parse("https://localhost:8080").expect("static URL is valid")
            });
        let bearer = SecretString::new(std::env::var("OCTOS_BEARER").unwrap_or_default());
        let profile_id = TransportProfileId::new(
            std::env::var("OCTOS_PROFILE_ID").unwrap_or_else(|_| "default".to_string()),
        );
        TransportConfig {
            base_url,
            bearer,
            profile_id,
            cursor: None,
            requested_capabilities: Capabilities::requested(),
            workspace_cwd: Self::current_workspace_cwd(),
            stdio: Self::stdio_spawn(),
        }
    }

    /// Build the stdio-transport spawn spec. On Android the app runs the
    /// bundled `octos` binary as `serve --stdio` instead of dialing a
    /// WebSocket: no `octos serve` daemon, no TCP port. `untrusted_app` can
    /// only exec from its nativeLibraryDir, so the binary must ship there as a
    /// `lib*.so`; we locate that dir from our own mapped `libmakepad.so`.
    /// `HOME` points at an app-private octos home whose
    /// `.config/octos/config.json` carries the provider + inline key — so the
    /// app process never holds the LLM secret. Returns `None` (⇒ WebSocket) on
    /// desktop, or on Android when the bundled binary is absent (safe
    /// fallback: the app still boots against a remote `octos serve`).
    #[cfg(target_os = "android")]
    fn stdio_spawn() -> Option<StdioSpawn> {
        let lib_dir = Self::android_native_lib_dir()?;
        let program = lib_dir.join("liboctos.so");
        if !program.exists() {
            log::warn!(
                "stdio: bundled octos not found at {}; using WebSocket transport",
                program.display()
            );
            return None;
        }
        let home = std::path::PathBuf::from("/data/user/0/dev.makepad.octos_app/files/octos-home");
        log::info!("stdio: octos={} HOME={}", program.display(), home.display());
        Some(StdioSpawn {
            program,
            args: vec!["serve".to_owned(), "--stdio".to_owned()],
            env: vec![("HOME".to_owned(), home.to_string_lossy().into_owned())],
            cwd: Some(home),
        })
    }

    #[cfg(not(target_os = "android"))]
    fn stdio_spawn() -> Option<StdioSpawn> {
        // Desktop dev keeps the WebSocket transport (talk to `octos serve`).
        None
    }

    /// Locate the app's nativeLibraryDir by scanning `/proc/self/maps` for our
    /// own already-mapped `libmakepad.so` — avoids a JNI round-trip to
    /// `ApplicationInfo.nativeLibraryDir` (the path carries a per-install hash,
    /// so it can't be hard-coded).
    #[cfg(target_os = "android")]
    fn android_native_lib_dir() -> Option<std::path::PathBuf> {
        let maps = std::fs::read_to_string("/proc/self/maps").ok()?;
        for line in maps.lines() {
            let Some(slash) = line.find('/') else { continue };
            let path = &line[slash..];
            if path.ends_with("/libmakepad.so") {
                return std::path::Path::new(path).parent().map(|p| p.to_path_buf());
            }
        }
        None
    }

    fn current_workspace_cwd() -> Option<String> {
        // Android: the process cwd is `/`, which the server's session
        // workspace policy rejects ("failed to bootstrap session workspace
        // policy"). No meaningful workspace exists on-device — omit it and
        // let the server pick the profile default.
        #[cfg(target_os = "android")]
        {
            None
        }
        #[cfg(not(target_os = "android"))]
        {
            std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
                .filter(|p| p != "/")
        }
    }

    /// Resolve the bearer token for `(host, profile_id)`. `OCTOS_APP_TOKEN`
    /// wins (`keychain::load_token` already honours it as a bypass), the
    /// keychain entry is consulted next, and an empty `SecretString` is
    /// returned otherwise so the caller still has a syntactically-valid
    /// `TransportConfig` (the REST round-trip 401s, which we surface
    /// silently in M1).
    fn resolve_bearer(base_url: &url::Url, profile_id_str: &str) -> SecretString {
        let host = octos_app_store::auth::ServerHost::from(
            crate::app::login::host_from_url(base_url),
        );
        let pid = ProfileId::from(profile_id_str.to_owned());
        match octos_app_store::keychain::load_token(&host, &pid) {
            Ok(Some(tok)) => SecretString::new(tok.expose().to_owned()),
            Ok(None) => SecretString::new(String::new()),
            Err(e) => {
                log::warn!("keychain load_token failed ({e}); using empty bearer");
                SecretString::new(String::new())
            }
        }
    }

    fn clear_chat(&mut self, cx: &mut Cx) {
        {
            let mut data = CHAT_DATA.write().unwrap();
            data.messages.clear();
            data.streaming_text.clear();
            data.thinking_text.clear();
            data.is_streaming = false;
            data.a2app_state.clear();
            data.save_to_disk();
        }
        // New session — the Splash manual must be re-primed into it.
        self.splash_primed = false;
        // Back to the compose state (no card on screen).
        self.composer_shown = true;
        self.sync_composer(cx);

        if let Some(agent) = &mut self.agent {
            let config = SessionConfig {
                system_prompt: Some(OCTOS_PLACEHOLDER_SYSTEM_PROMPT.to_string()),
                ..Default::default()
            };
            self.session_id = Some(agent.create_session(cx, config));
        }
        self.update_empty_state_visibility(cx);
        self.ui.redraw(cx);
    }

    fn update_empty_state_visibility(&self, cx: &mut Cx) {
        let (show_empty_state, is_streaming) = {
            let data = CHAT_DATA.read().unwrap();
            (
                data.messages.is_empty() && !data.is_streaming,
                data.is_streaming,
            )
        };
        self.ui
            .view(cx, ids!(empty_state))
            .set_visible(cx, show_empty_state);
        // Swimming octopus = "the model is working on it".
        self.ui
            .view(cx, ids!(octo_row))
            .set_visible(cx, is_streaming);
    }

    fn send_message(&mut self, cx: &mut Cx) {
        let input = self.ui.text_input(cx, ids!(input));
        let text = input.text();
        if text.trim().is_empty() {
            return;
        }

        if self.agent.is_none() || self.session_id.is_none() {
            return;
        }

        let items_len = {
            let mut data = CHAT_DATA.write().unwrap();
            data.messages.push(ChatMessage {
                role: ChatRole::User,
                text: text.clone(),
            });
            data.streaming_text.clear();
            data.thinking_text.clear();
            data.is_streaming = true;
            data.messages.len() + 1
        };
        input.set_text(cx, "");
        self.update_empty_state_visibility(cx);

        let session_id = self.session_id.unwrap();
        let agent = self.agent.as_mut().unwrap();

        // Octos sessions are stateful server-side, so we don't inject
        // history client-side (aichat's stateless replay is gone — see
        // `05-AICHAT-REUSE-MAP.md` "Stuff we drop or replace").
        //
        // Splash mode: the bubble shows the user's original `text`, but the
        // LLM receives the Splash UI-generation prompt + manual so it returns
        // a `runsplash` block the Markdown widget renders live.
        let sent = if self.splash_mode {
            let base = if self.splash_primed {
                // Manual already in session history — send a short directive.
                app_splash_followup(&text)
            } else {
                self.splash_primed = true;
                app_splash_prompt(&text)
            };
            // Attach the user's saved named cards so the model can retrieve and
            // refine one by name ("improve the weather card" → the weather-sf
            // card's DSL is right here to edit and re-emit).
            let saved = load_a2app_cards(6);
            log::info!("a2app: injecting {} saved card(s) into prompt", saved.len());
            if saved.is_empty() {
                base
            } else {
                let mut lib = String::from(
                    "\n\nYOUR SAVED CARDS — if this request refines/improves/changes one of \
these, edit that card and return the FULL updated block KEEPING its exact \
`// name:` line:\n",
                );
                for (name, dsl) in &saved {
                    lib.push_str(&format!("\n[{name}]\n```runsplash\n{}\n```\n", dsl.trim()));
                }
                format!("{base}{lib}")
            }
        } else {
            text.clone()
        };
        self.current_prompt = Some(agent.send_prompt(cx, session_id, &sent));
        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, true);

        let chat_list = self.ui.widget(cx, ids!(chat_list));
        let list = chat_list.portal_list(cx, ids!(list));
        list.set_tail_range(true);
        list.set_first_id_and_scroll(items_len.saturating_sub(1), 0.0);
        self.ui.redraw(cx);
    }

    fn cancel_request(&mut self, cx: &mut Cx) {
        if let (Some(agent), Some(prompt_id)) = (&mut self.agent, self.current_prompt.take()) {
            agent.cancel_prompt(cx, prompt_id);

            let mut data = CHAT_DATA.write().unwrap();
            let text = std::mem::take(&mut data.streaming_text);
            data.thinking_text.clear();
            if !text.is_empty() {
                data.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    text,
                });
            }
            data.is_streaming = false;
            drop(data);

            self.update_empty_state_visibility(cx);
            self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
            self.ui.redraw(cx);
        }
    }

    /// Reflect `composer_shown` into the floating composer + reveal pill: when
    /// expanded the glass composer shows and the pill hides; when collapsed
    /// (after a card renders) only the slim pill shows, giving the card the
    /// full screen. A full redraw is required after flipping glass-composite
    /// visibility or the old composite lingers (see [[octos-app-android]]).
    fn sync_composer(&mut self, cx: &mut Cx) {
        let show = self.composer_shown;
        self.ui.widget(cx, ids!(composer)).set_visible(cx, show);
        self.ui.button(cx, ids!(reveal_pill)).set_visible(cx, !show);
        cx.redraw_all();
    }

    /// Status label content. W01 will rewrite this to show `Connected ·
    /// {latency}ms · cursor {seq}` per `04-IA-AND-NAVIGATION.md` §
    /// "Top bar contents"; for now it reflects whether a profile has been
    /// selected.
    fn update_status(&self, cx: &mut Cx) {
        let status = match self.current_profile.as_ref() {
            Some(profile) => format!("Connected · profile={}", profile),
            None => "Initializing...".to_string(),
        };
        self.ui.label(cx, ids!(status_label)).set_text(cx, &status);
    }

    /// W04 follow-up #3 — render `APP_STATE.connection` as the top-bar
    /// status dot + label. Green = Connected, amber = Reconnecting, red =
    /// Offline. Pure read of `AppState` mirrored by `OctosUiAgent` on
    /// `TransportEvent::ConnectionState`.
    fn update_connection_indicator(&self, cx: &mut Cx) {
        use octos_app_store::state::ConnectionState as StoreCs;
        let cs = APP_STATE
            .read()
            .map(|s| s.connection)
            .unwrap_or(StoreCs::Offline);
        let (label, color) = match cs {
            StoreCs::Connected => ("Live", "#x4FCB6E"),
            StoreCs::Reconnecting => ("Reconnecting", "#xF6BE63"),
            StoreCs::Offline => ("Offline", "#xE36363"),
        };
        let _ = color; // referenced in the script_apply_eval below
        self.ui
            .label(cx, ids!(connection_state_label))
            .set_text(cx, label);
        let mut dot = self.ui.label(cx, ids!(connection_dot));
        match cs {
            StoreCs::Connected => script_apply_eval!(cx, dot, {
                draw_text +: { color: #x4FCB6E }
            }),
            StoreCs::Reconnecting => script_apply_eval!(cx, dot, {
                draw_text +: { color: #xF6BE63 }
            }),
            StoreCs::Offline => script_apply_eval!(cx, dot, {
                draw_text +: { color: #xE36363 }
            }),
        }
    }

    /// Re-render every assistant message's markdown with the current A2App
    /// counter substituted into `{{state.count}}`. Mirrors aichat's
    /// `refresh_visible_state_templates`: set_text directly on each pooled
    /// PortalList item's markdown (a plain redraw does NOT re-run the item's
    /// draw), so a live counter updates in place.
    fn refresh_a2app_templates(&self, cx: &mut Cx) {
        let messages: Vec<(usize, String, CardState)> = {
            let data = CHAT_DATA.read().unwrap();
            data.messages
                .iter()
                .enumerate()
                .filter_map(|(i, m)| match m.role {
                    ChatRole::Assistant => Some((
                        i,
                        m.text.clone(),
                        data.a2app_state.get(&i).cloned().unwrap_or_default(),
                    )),
                    _ => None,
                })
                .collect()
        };
        let chat_list = self.ui.widget(cx, ids!(chat_list));
        let list = chat_list.portal_list(cx, ids!(list));
        for (item_id, text, state) in messages {
            if let Some((_, item)) = list.get_item(item_id) {
                // Re-feed the whole markdown (keeps non-splash content current).
                let unwrapped = unwrap_outer_markdown_fence(&text);
                let rendered = wrap_bare_latex(unwrapped);
                let rendered = resolve_a2app_card(&rendered, item_id, &state);
                item.markdown(cx, ids!(selectable)).set_text(cx, &rendered);
                // Also push the resolved `runsplash` body straight to the
                // Splash widget — its `set_text` re-evals on change, and this
                // guarantees the update even if the markdown re-parse doesn't
                // re-dispatch to the pooled splash_view.
                if let Some(body) = extract_runsplash_body(&text) {
                    let resolved = substitute_card_state(body, item_id, &state);
                    item.widget(cx, ids!(splash_view)).set_text(cx, &resolved);
                }
            }
        }
        cx.redraw_all();
    }

    /// Drive the toast strip from `APP_STATE.toasts`. Shows the front
    /// (oldest) queued toast for a few seconds, then the timer dismisses it
    /// and advances to the next. No-op while a toast is already on screen
    /// (`toast_timer` non-empty).
    fn sync_toasts(&mut self, cx: &mut Cx) {
        if !self.toast_timer.is_empty() {
            return;
        }
        let front = APP_STATE
            .read()
            .ok()
            .and_then(|s| s.toasts.iter().next().cloned());
        match front {
            Some(t) => {
                self.ui.label(cx, ids!(toast_label)).set_text(cx, &t.message);
                self.ui.view(cx, ids!(toast_row)).set_visible(cx, true);
                self.toast_timer = cx.start_timeout(3.8);
                cx.redraw_all();
            }
            None => {
                self.ui.view(cx, ids!(toast_row)).set_visible(cx, false);
            }
        }
    }

    /// Top-bar context-usage chip. Reads `APP_STATE.context` (updated every
    /// turn from `context/normalization`) and shows the model context-window
    /// fill — e.g. `◔ 10k · 68 msgs`. Blank until the first turn reports.
    fn update_context_indicator(&self, cx: &mut Cx) {
        let ctx = APP_STATE.read().ok().and_then(|s| s.context.clone());
        let text = match ctx {
            Some(c) => {
                let tok = c.token_estimate;
                let tok_str = if tok >= 1000 {
                    format!("{:.1}k", tok as f64 / 1000.0)
                } else {
                    format!("{tok}")
                };
                format!("\u{25D4} {tok_str} \u{00B7} {} msgs", c.item_count)
            }
            None => String::new(),
        };
        self.ui.label(cx, ids!(context_chip)).set_text(cx, &text);
    }

    fn apply_glass_opacity(&self, cx: &mut Cx, opacity: f64) {
        let opacity = opacity.clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY);
        let glass = glass_opacity_values(opacity);

        let mut app_shell = self.ui.view(cx, ids!(app_shell));
        script_apply_eval!(cx, app_shell, {
            draw_bg +: { tint_alpha: #(glass.app) }
        });

        let mut sidebar = self.ui.view(cx, ids!(sidebar));
        script_apply_eval!(cx, sidebar, {
            draw_bg +: { tint_alpha: #(glass.sidebar) }
        });

        let mut main_area = self.ui.view(cx, ids!(main_area));
        script_apply_eval!(cx, main_area, {
            draw_bg +: { tint_alpha: #(glass.main) }
        });

        let mut composer = self.ui.view(cx, ids!(composer));
        script_apply_eval!(cx, composer, {
            draw_bg +: { tint_alpha: #(glass.composer) }
        });

        self.ui
            .label(cx, ids!(opacity_value))
            .set_text(cx, &format!("{:.0}%", opacity * 100.0));
        self.ui.redraw(cx);
    }

    // ---- W04 / M2 — Content + Viewers helpers --------------------------

    /// Flip the active screen sibling based on `APP_STATE.navigation`.
    /// Mirrors `show_login`'s lockstep `set_visible` pattern; W06 added
    /// `coding_screen` and W07 added `studio/slides/sites_screen`.
    fn show_screen_for_nav(&self, cx: &mut Cx) {
        let nav = APP_STATE
            .read()
            .map(|s| s.navigation.clone())
            .unwrap_or_default();
        let is_content = matches!(nav, CurrentScreen::Content);
        // Chat is the implicit default — show it for any other navigation
        // state (incl. the removed Coding / Studio / Slides / Sites states,
        // should the store ever carry them).
        let is_chat = !is_content;
        self.ui
            .view(cx, ids!(chat_screen))
            .set_visible(cx, is_chat);
        self.ui
            .view(cx, ids!(content_screen))
            .set_visible(cx, is_content);
        self.ui.redraw(cx);
    }

    /// Sidebar `nav_content` click — flip to Content + fire REST hydrate.
    fn navigate_to_content(&mut self, cx: &mut Cx) {
        {
            let mut state = APP_STATE.write().unwrap();
            octos_app_store::state::reduce(
                &mut state,
                octos_app_store::state::Event::Navigation(
                    NavigationEvent::NavigateTo(CurrentScreen::Content),
                ),
            );
        }
        self.show_screen_for_nav(cx);
        self.fire_content_hydrate();
    }

    // navigate_to_coding / navigate_to_producer removed with the Coding /
    // Studio / Slides / Sites navs (unsupported in this build).

    /// Phone-width helper: the desktop shell keeps sidebar and chat side by
    /// side, which pushes the chat off-screen on a portrait phone. Collapse
    /// the sidebar after sidebar-driven navigation when the window is
    /// narrow; the top-bar ☰ button brings it back.
    fn collapse_sidebar_if_narrow(&self, cx: &mut Cx) {
        let w = self
            .ui
            .window(cx, ids!(main_window))
            .get_inner_size(cx)
            .x;
        if w > 0.0 && w < 600.0 {
            self.ui.view(cx, ids!(sidebar)).set_visible(cx, false);
            // The glass-opacity toolbar is a desktop nicety; its 318pt
            // fixed width alone overflows a phone top bar.
            self.ui.view(cx, ids!(glass_toolbar)).set_visible(cx, false);
            cx.redraw_all();
        }
    }

    /// Spawn an off-thread `task/output/read` and post the reply back as
    /// `TaskOutputAction`. Same lifecycle shape as `hydrate_sessions`
    /// (`app/src/app/sessions.rs:hydrate_sessions`) — short-lived
    /// `current_thread` runtime so the call site doesn't need to already
    /// be inside one.
    ///
    /// We can't reach the WS transport from this thread (it's owned by
    /// the agent's own runtime); instead we hop through the REST
    /// fallback path the agent uses for one-shot reads. For M3 we keep
    /// it simple and synthesize the call via the WS handle if available.
    fn fire_task_output_read(&self, task_id: octos_core::TaskId) {
        // Resolve the session id from APP_STATE — without it, the wire
        // params are invalid. Bail silently if no session is open.
        let Some(session_id) = APP_STATE
            .read()
            .ok()
            .and_then(|s| s.current_session.clone())
        else {
            return;
        };
        let params =
            crate::app::coding::build_output_read_params(session_id.clone(), task_id.clone());
        if let Some(handle) = self.task_output_handle.as_ref() {
            handle.read(params);
        } else {
            Cx::post_action(crate::app::coding::TaskOutputAction {
                task_id,
                session_id,
                outcome: crate::app::coding::TaskOutputOutcome::Failed(
                    "agent not initialized".to_owned(),
                ),
            });
        }
    }

    /// Spawn the off-thread REST hydrate. Reads filter / search from
    /// `CONTENT_STATE` (server-side `kind` / `q`).
    fn fire_content_hydrate(&self) {
        let cfg = Self::placeholder_transport_config();
        let client = Self::build_rest_client(&cfg);
        let (kind, q) = CONTENT_STATE
            .read()
            .ok()
            .map(|cs| {
                (
                    cs.filter.server_kind().map(|s| s.to_owned()),
                    if cs.search.trim().is_empty() {
                        None
                    } else {
                        Some(cs.search.trim().to_owned())
                    },
                )
            })
            .unwrap_or((None, None));
        content_mod::hydrate_content(client, MyContentQuery {
            kind,
            q,
            limit: None,
            cursor: None,
        });
    }

    /// Open the right viewer for `handle`. Markdown additionally fires a
    /// background `reqwest` for the body unless cached.
    fn open_viewer_for(&self, cx: &mut Cx, handle: octos_app_store::files::FileHandle) {
        let open = viewers_mod::viewer_for(&handle);
        let need_md_fetch = matches!(open, OpenViewer::Markdown { .. })
            && VIEWER_STATE
                .read()
                .map(|vs| !vs.markdown_cache.contains_key(&handle))
                .unwrap_or(true);
        if let Ok(mut vs) = VIEWER_STATE.write() {
            vs.open = open;
            vs.last_error = None;
        }
        if need_md_fetch {
            let cfg = Self::placeholder_transport_config();
            let client = Self::build_rest_client(&cfg);
            viewers_mod::fetch_markdown(client, handle);
        }
        // Full repaint — overlay visibility flip (see `show_login`).
        cx.redraw_all();
    }

    fn close_viewer(&self, cx: &mut Cx) {
        if let Ok(mut vs) = VIEWER_STATE.write() {
            vs.open = OpenViewer::Closed;
        }
        // Full repaint — overlay visibility flip (see `show_login`).
        cx.redraw_all();
    }

    /// Image album prev/next — clamps to [0, len).
    fn album_step(&self, cx: &mut Cx, delta: i32) {
        if let Ok(mut vs) = VIEWER_STATE.write() {
            if let OpenViewer::ImageAlbum { handles, active } = &mut vs.open {
                if !handles.is_empty() {
                    let len = handles.len() as i32;
                    let next = (*active as i32 + delta).clamp(0, len - 1);
                    *active = next as usize;
                }
            }
        }
        self.ui.redraw(cx);
    }

    /// Use `robius_open` to launch the OS default viewer for the handle.
    fn open_in_os(&self, handle: &octos_app_store::files::FileHandle) {
        let cfg = Self::placeholder_transport_config();
        let client = Self::build_rest_client(&cfg);
        let Some(url) = viewers_mod::url_for(&client, handle) else {
            log::warn!("open_in_os: file_url failed for {handle}");
            return;
        };
        if let Err(e) = robius_open::Uri::new(url.as_str()).open() {
            log::warn!("robius_open {handle}: {e:?}");
        }
    }

    // ---- W08 — login flow helpers ------------------------------------------

    /// Toggle between the LoginScreen overlay and the chat shell. Lockstep
    /// `set_visible` on `app_shell` and `login_overlay` so only one is
    /// interactive at a time.
    fn show_login(&self, cx: &mut Cx, show: bool) {
        self.ui.view(cx, ids!(app_shell)).set_visible(cx, !show);
        self.ui.view(cx, ids!(login_overlay)).set_visible(cx, show);
        // Full repaint, not just `ui.redraw`: the glass widgets draw into
        // self-managed overlay draw lists, and a partial redraw can leave a
        // stale composite on screen after a visibility flip (on Android this
        // showed as a black boot screen / a login card that never dismissed —
        // same failure mode aichat documents in its `clear_chat`).
        cx.redraw_all();
    }

    /// Push a status / error string to the LoginScreen status label. Empty
    /// string clears the surface (used after a successful step).
    fn login_set_status(&self, cx: &mut Cx, msg: &str) {
        self.ui
            .label(cx, ids!(login_status_label))
            .set_text(cx, msg);
    }

    /// Boot-time decision: are we already authed? Honours
    /// `OCTOS_APP_TOKEN` (dev shortcut) > server.json + keychain > go to
    /// Login. Side-effect: caches `login_server_url` / `login_profile_id`
    /// from the config file when present so the email / verify steps don't
    /// have to re-read disk.
    fn boot_is_authed(&mut self) -> bool {
        if let Ok(t) = std::env::var("OCTOS_APP_TOKEN") {
            if !t.is_empty() {
                log::info!("OCTOS_APP_TOKEN present; skipping LoginScreen");
                return true;
            }
        }
        let Some(cfg) = crate::app::login::load_server_config() else {
            log::info!("no server.json — starting at LoginScreen Step 1");
            return false;
        };
        let url = match url::Url::parse(&cfg.server_url) {
            Ok(u) => u,
            Err(e) => {
                log::warn!("server.json has invalid URL ({e}); falling back to Login");
                return false;
            }
        };
        let host = octos_app_store::auth::ServerHost::from(
            crate::app::login::host_from_url(&url),
        );
        let pid = ProfileId::from(cfg.profile_id.clone());
        self.login_server_url = Some(url);
        self.login_profile_id = Some(pid.clone());
        match octos_app_store::keychain::load_token(&host, &pid) {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                log::warn!("keychain load failed ({e}); falling back to Login");
                false
            }
        }
    }

    /// Step 1 — `Continue` button. Validates the server URL, persists
    /// `~/.config/octos-app/server.json`, hides Step 1 and shows Step 2.
    fn login_continue_clicked(&mut self, cx: &mut Cx) {
        let url_str = self.ui.text_input(cx, ids!(login_server_url_input)).text();
        let pid_str = self.ui.text_input(cx, ids!(login_profile_id_input)).text();
        let pid_trimmed = pid_str.trim();
        if pid_trimmed.is_empty() {
            self.login_set_status(cx, "Profile ID is required");
            return;
        }
        let parsed = match crate::app::login::validate_server_url(&url_str) {
            Ok(u) => u,
            Err(e) => {
                self.login_set_status(cx, &e);
                return;
            }
        };
        let cfg = crate::app::login::ServerConfig {
            server_url: parsed.to_string(),
            profile_id: pid_trimmed.to_string(),
        };
        if let Err(e) = crate::app::login::save_server_config(&cfg) {
            self.login_set_status(cx, &format!("Failed to save server config: {e}"));
            return;
        }
        self.login_server_url = Some(parsed.clone());
        self.login_profile_id = Some(ProfileId::from(pid_trimmed.to_string()));
        self.ui
            .view(cx, ids!(login_server_step))
            .set_visible(cx, false);
        // Before falling back to the email OTP flow, try the password-free
        // solo sign-in that `octos serve --solo` exposes (same flow as
        // octos-web's local sign-in button). The email step only appears if
        // solo is unavailable (SoloReply handler below).
        self.login_set_status(cx, "Trying password-free sign-in…");
        self.ui.redraw(cx);
        let url = parsed;
        let pid = ProfileId::from(pid_trimmed.to_string());
        std::thread::spawn(move || {
            let outcome = run_blocking_solo_login(&url, &pid);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::SoloReply,
                error: outcome.err(),
            });
        });
    }

    /// Step 2 — `Send code` button. Drives `POST /api/auth/send-code`
    /// (octos-cli auth_handlers.rs:389). Server always returns `ok: true`
    /// (per the design note about preventing email-enumeration), so on a
    /// non-transport response we unconditionally advance to Step 3.
    fn login_send_code_clicked(&mut self, cx: &mut Cx) {
        let email = self.ui.text_input(cx, ids!(login_email_input)).text();
        let trimmed = email.trim().to_string();
        if trimmed.is_empty() || !trimmed.contains('@') {
            self.login_set_status(cx, "Enter a valid email address");
            return;
        }
        let Some(server_url) = self.login_server_url.clone() else {
            self.login_set_status(cx, "No server configured (Step 1)");
            return;
        };
        self.login_pending_email = Some(trimmed.clone());
        self.login_set_status(cx, "Sending code...");
        self.ui.redraw(cx);

        // Off-thread REST call: the UI thread cannot host an async runtime
        // (Makepad owns the event loop), so we build a one-shot
        // single-threaded tokio runtime on a worker thread, run the call,
        // and post a typed action back via `Cx::post_action`. No global
        // runtime, no shared state — the worker dies once it's posted.
        std::thread::spawn(move || {
            let result = run_blocking_send_code(&server_url, &trimmed);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::SendCodeReply,
                error: result.err(),
            });
        });
    }

    /// Step 3 — `Verify` button. Drives `POST /api/auth/verify`
    /// (octos-cli auth_handlers.rs:543). On `ok && token` the keychain
    /// stores the bearer keyed under `<host>::<profile_id>`.
    fn login_verify_clicked(&mut self, cx: &mut Cx) {
        let code = self.ui.text_input(cx, ids!(login_code_input)).text();
        let trimmed = code.trim().to_string();
        if trimmed.is_empty() {
            self.login_set_status(cx, "Enter the verification code");
            return;
        }
        let Some(server_url) = self.login_server_url.clone() else {
            self.login_set_status(cx, "No server configured (Step 1)");
            return;
        };
        let Some(pid) = self.login_profile_id.clone() else {
            self.login_set_status(cx, "No profile id configured (Step 1)");
            return;
        };
        let Some(email) = self.login_pending_email.clone() else {
            self.login_set_status(cx, "Send a code first");
            return;
        };
        self.login_set_status(cx, "Verifying...");
        self.ui.redraw(cx);

        std::thread::spawn(move || {
            let outcome = run_blocking_verify(&server_url, &email, &trimmed, &pid);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::VerifyReply,
                error: outcome.err(),
            });
        });
    }

    /// `Sign out` — clear keychain + reset the LoginScreen step state +
    /// flip the overlay back on. Server-side `/api/auth/logout`
    /// (auth_handlers.rs:680) is not yet plumbed; the bearer becomes
    /// invalid client-side regardless.
    fn login_sign_out(&mut self, cx: &mut Cx) {
        if let (Some(url), Some(pid)) = (
            self.login_server_url.clone(),
            self.login_profile_id.clone(),
        ) {
            let host = octos_app_store::auth::ServerHost::from(
                crate::app::login::host_from_url(&url),
            );
            if let Err(e) = octos_app_store::keychain::delete_token(&host, &pid) {
                log::warn!("delete_token failed (continuing logout): {e}");
            }
        }
        self.login_pending_email = None;
        // Login-free flow: dropping the bearer just re-provisions in the
        // background (fresh solo identity/token); the shell stays up.
        self.auto_solo_login(cx);
    }

    /// Background password-free sign-in. Ensures a server config exists
    /// (default: the on-device solo server) and spawns the solo attempt;
    /// the reply lands as `LoginAsyncEvent::SoloReply` in `handle_actions`.
    fn auto_solo_login(&mut self, cx: &mut Cx) {
        const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:50080";
        const DEFAULT_PROFILE: &str = "octos";
        if crate::app::login::load_server_config().is_none() {
            let cfg = crate::app::login::ServerConfig {
                server_url: DEFAULT_SERVER_URL.to_string(),
                profile_id: DEFAULT_PROFILE.to_string(),
            };
            if let Err(e) = crate::app::login::save_server_config(&cfg) {
                log::warn!("auto-solo: save default server config: {e}");
            }
        }
        let Some(cfg) = crate::app::login::load_server_config() else {
            return;
        };
        let Ok(url) = url::Url::parse(&cfg.server_url) else {
            log::warn!("auto-solo: bad server_url in config");
            return;
        };
        let pid = ProfileId::from(cfg.profile_id.clone());
        self.login_server_url = Some(url.clone());
        self.login_profile_id = Some(pid.clone());
        self.ui
            .label(cx, ids!(status_label))
            .set_text(cx, "Signing in…");
        std::thread::spawn(move || {
            let outcome = run_blocking_solo_login(&url, &pid);
            Cx::post_action(LoginAsyncAction {
                kind: LoginAsyncEvent::SoloReply,
                error: outcome.err(),
            });
        });
    }
}

// ---------------------------------------------------------------------------
// Off-thread helpers for the LoginScreen REST calls. Each call builds a
// one-shot single-threaded tokio runtime on a `std::thread::spawn` worker,
// runs the call, and posts a typed `LoginAsyncAction` back via
// `Cx::post_action`. No global runtime, no shared state.

fn run_blocking_send_code(server_url: &url::Url, email: &str) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(async move {
        let client = octos_app_transport::rest::RestClient::new(
            reqwest::Client::new(),
            server_url.clone(),
            octos_app_transport::SecretString::new(""),
            octos_app_transport::ProfileId::new(""),
        );
        client
            .send_code(email)
            .await
            .map(|_| ())
            .map_err(|e| format!("send-code: {e}"))
    })
}

fn run_blocking_verify(
    server_url: &url::Url,
    email: &str,
    code: &str,
    profile_id: &ProfileId,
) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let host = octos_app_store::auth::ServerHost::from(
        crate::app::login::host_from_url(server_url),
    );
    let pid = profile_id.clone();
    rt.block_on(async move {
        let client = octos_app_transport::rest::RestClient::new(
            reqwest::Client::new(),
            server_url.clone(),
            octos_app_transport::SecretString::new(""),
            octos_app_transport::ProfileId::new(""),
        );
        let resp = client
            .verify(email, code)
            .await
            .map_err(|e| format!("verify: {e}"))?;
        if !resp.ok {
            return Err(resp
                .message
                .unwrap_or_else(|| "Server rejected the code".to_string()));
        }
        let token = resp
            .token
            .ok_or_else(|| "Server returned ok=true but no token".to_string())?;
        let secret = octos_app_store::auth::SecretToken::from(token);
        octos_app_store::keychain::store_token(&host, &pid, &secret)
            .map_err(|e| format!("store_token: {e}"))
    })
}

/// Password-free sign-in against a server running `octos serve --solo`:
/// `POST /api/auth/solo` re-login first, then `POST /api/auth/solo/create`
/// on 404 (no solo owner yet) — mirroring octos-web's local sign-in. Stores
/// the bearer under the same keychain key the OTP flow uses.
fn run_blocking_solo_login(
    server_url: &url::Url,
    profile_id: &ProfileId,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct SoloUserLite {
        id: String,
    }
    #[derive(serde::Deserialize)]
    struct SoloCreateLite {
        profile_id: String,
    }
    #[derive(serde::Deserialize)]
    struct SoloTokenResp {
        token: String,
        // `POST /api/auth/solo` re-login returns the existing owner; adopt
        // its id so the bearer keys/config match the server's identity even
        // when the local default profile guess differs.
        #[serde(default)]
        user: Option<SoloUserLite>,
        #[serde(default)]
        result: Option<SoloCreateLite>,
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let host = octos_app_store::auth::ServerHost::from(
        crate::app::login::host_from_url(server_url),
    );
    let pid = profile_id.clone();
    rt.block_on(async move {
        let client = reqwest::Client::new();
        let login_url = server_url
            .join("api/auth/solo")
            .map_err(|e| format!("solo url: {e}"))?;
        let resp = client
            .post(login_url)
            .send()
            .await
            .map_err(|e| format!("solo sign-in: {e}"))?;
        let parsed = match resp.status().as_u16() {
            200 => resp
                .json::<SoloTokenResp>()
                .await
                .map_err(|e| format!("solo response: {e}"))?,
            404 => {
                // No solo owner yet — create it (server must be in --solo
                // mode; anything else 403s below).
                let create_url = server_url
                    .join("api/auth/solo/create")
                    .map_err(|e| format!("solo create url: {e}"))?;
                let body = serde_json::json!({
                    "name": pid.as_str(),
                    "username": pid.as_str(),
                    "email": format!("{}@octos.local", pid.as_str()),
                });
                let resp = client
                    .post(create_url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| format!("solo create: {e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("solo create: HTTP {}", resp.status()));
                }
                resp.json::<SoloTokenResp>()
                    .await
                    .map_err(|e| format!("solo create response: {e}"))?
            }
            403 => return Err("Solo sign-in is disabled on this server".to_string()),
            s => return Err(format!("solo sign-in: HTTP {s}")),
        };
        // Adopt the server's owner identity (re-login returns the existing
        // solo owner even when our local profile guess differs) and keep the
        // on-disk config in lockstep so `resolve_bearer` finds the token.
        let owner = parsed
            .user
            .map(|u| u.id)
            .or(parsed.result.map(|r| r.profile_id))
            .unwrap_or_else(|| pid.as_str().to_owned());
        let owner_pid = octos_app_store::auth::ProfileId::from(owner.clone());
        let secret = octos_app_store::auth::SecretToken::from(parsed.token);
        octos_app_store::keychain::store_token(&host, &owner_pid, &secret)
            .map_err(|e| format!("store_token: {e}"))?;
        let _ = crate::app::login::save_server_config(&crate::app::login::ServerConfig {
            server_url: server_url.to_string(),
            profile_id: owner,
        });
        Ok(())
    })
}

/// Discriminator for cross-thread login replies. Carrying all arms through
/// one `ActionTrait` (auto-derived from `Debug + 'static` per
/// `aichat/platform/src/action.rs:21`) keeps the `Cx::post_action`
/// boilerplate down.
#[derive(Clone, Copy, Debug)]
enum LoginAsyncEvent {
    SendCodeReply,
    VerifyReply,
    /// Password-free `--solo` attempt fired by the Step-1 `Continue` button.
    SoloReply,
}

#[derive(Debug)]
struct LoginAsyncAction {
    kind: LoginAsyncEvent,
    error: Option<String>,
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let opacity_slider = self.ui.slider(cx, ids!(opacity_slider));
        if let Some(opacity) = opacity_slider
            .slided(actions)
            .or_else(|| opacity_slider.end_slide(actions))
        {
            self.apply_glass_opacity(cx, opacity);
        }
        // Thinking + A2App toggles were removed (always-on A2App card app).

        // Reveal pill → expand the floating composer again after it auto-hid.
        if self.ui.button(cx, ids!(reveal_pill)).clicked(actions) {
            self.composer_shown = true;
            self.sync_composer(cx);
        }

        // Markdown link click — dispatch through robius-open for cross-platform
        // coverage (macOS/Linux/Windows/iOS/Android/WASM). Desktop requires a
        // modifier (Cmd on macOS, Cmd/Ctrl elsewhere) so plain clicks stay
        // available for drag-selection inside the Markdown widget; mobile &
        // web have no modifier concept, so a plain tap opens the URL.
        for action in actions {
            // Button press inside LLM-generated A2App/Splash UI. Update the
            // live counter from common event names and redraw so the
            // `{{state.count}}` placeholder reflects the new value; also toast
            // the action so any event is visibly acknowledged.
            if let makepad_widgets::SplashAction::Notify { event_id, payload } = action.cast() {
                // event_id is tagged "<card_id>:<event>" (see `tag_notify_calls`)
                // so the press routes to the card that fired it; `payload` is
                // JSON, optionally {"key": "<name>", "value": "<string>"}.
                let (card_id, ev) = match event_id.split_once(':') {
                    Some((id, rest)) => (id.parse::<usize>().ok(), rest.to_lowercase()),
                    None => (None, event_id.to_lowercase()),
                };
                let pj: serde_json::Value =
                    serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
                let key = pj.get("key").and_then(|v| v.as_str()).unwrap_or("count").to_owned();
                let value = pj.get("value").and_then(|v| v.as_str()).map(str::to_owned);
                let mut changed = false;
                if let Some(card_id) = card_id {
                    if let Ok(mut data) = CHAT_DATA.write() {
                        let card = data.a2app_state.entry(card_id).or_default();
                        let cur = |c: &CardState| -> i64 {
                            c.get(&key).and_then(|s| s.parse().ok()).unwrap_or(0)
                        };
                        changed = true;
                        if ev.contains("inc") || ev.contains("plus") || ev.contains("add") {
                            let n = cur(card);
                            card.insert(key.clone(), (n + 1).to_string());
                        } else if ev.contains("dec") || ev.contains("minus") || ev.contains("sub") {
                            let n = cur(card);
                            card.insert(key.clone(), (n - 1).to_string());
                        } else if ev.contains("reset") || ev.contains("clear") {
                            card.insert(key.clone(), "0".to_owned());
                        } else if ev.starts_with("set") {
                            // `set` last: "reset" also contains "set".
                            match value {
                                Some(v) => {
                                    card.insert(key.clone(), v);
                                }
                                None => changed = false,
                            }
                        } else {
                            changed = false;
                        }
                    }
                }
                if changed {
                    self.refresh_a2app_templates(cx);
                }
            }
            if let Some(widget_action) = action.as_widget_action() {
                if let makepad_widgets::markdown::MarkdownAction::LinkNavigated { url, modifiers } =
                    widget_action.cast()
                {
                    let should_open = {
                        #[cfg(any(
                            target_os = "ios",
                            target_os = "android",
                            target_arch = "wasm32"
                        ))]
                        {
                            let _ = modifiers;
                            true
                        }
                        #[cfg(not(any(
                            target_os = "ios",
                            target_os = "android",
                            target_arch = "wasm32"
                        )))]
                        {
                            modifiers.logo || modifiers.control
                        }
                    };
                    if should_open {
                        if let Err(e) = robius_open::Uri::new(&url).open() {
                            log::warn!("failed to open URL {}: {:?}", url, e);
                        }
                    }
                }
            }
        }
        if self.ui.button(cx, ids!(send_button)).clicked(actions) {
            self.send_message(cx);
        }
        if self.ui.button(cx, ids!(cancel_button)).clicked(actions) {
            self.cancel_request(cx);
        }
        if self.ui.button(cx, ids!(clear_button)).clicked(actions) {
            self.clear_chat(cx);
        }
        // Sidebar `+ 新对话` — same semantics as Clear: wipe the local chat
        // surface and open a fresh session on the wire. On phone-width
        // windows also collapse the sidebar so the chat surface (previously
        // pushed off-screen) becomes visible — this is what makes the button
        // *look* like it did something on a portrait phone.
        if self.ui.button(cx, ids!(nav_new)).clicked(actions) {
            self.clear_chat(cx);
            {
                let mut state = APP_STATE.write().unwrap();
                octos_app_store::state::reduce(
                    &mut state,
                    octos_app_store::state::Event::Navigation(
                        NavigationEvent::NavigateTo(CurrentScreen::Home),
                    ),
                );
            }
            self.show_screen_for_nav(cx);
            self.collapse_sidebar_if_narrow(cx);
        }
        // Top-bar ☰ — bring the collapsed sidebar back (or hide it again).
        if self.ui.button(cx, ids!(nav_toggle)).clicked(actions) {
            let sidebar = self.ui.view(cx, ids!(sidebar));
            let vis = sidebar.borrow().map(|v| v.visible()).unwrap_or(true);
            sidebar.set_visible(cx, !vis);
            cx.redraw_all();
        }
        if self
            .ui
            .text_input(cx, ids!(input))
            .returned(actions)
            .is_some()
        {
            self.send_message(cx);
        }
        if self.ui.text_input(cx, ids!(input)).escaped(actions) {
            self.cancel_request(cx);
        }

        // ---- W08 — LoginScreen buttons + Sign out -------------------------
        if self.ui.button(cx, ids!(login_continue_button)).clicked(actions) {
            self.login_continue_clicked(cx);
        }
        if self.ui.button(cx, ids!(login_send_code_button)).clicked(actions) {
            self.login_send_code_clicked(cx);
        }
        if self
            .ui
            .text_input(cx, ids!(login_email_input))
            .returned(actions)
            .is_some()
        {
            self.login_send_code_clicked(cx);
        }
        if self.ui.button(cx, ids!(login_verify_button)).clicked(actions) {
            self.login_verify_clicked(cx);
        }
        if self
            .ui
            .text_input(cx, ids!(login_code_input))
            .returned(actions)
            .is_some()
        {
            self.login_verify_clicked(cx);
        }
        if self.ui.button(cx, ids!(sign_out_button)).clicked(actions) {
            self.login_sign_out(cx);
        }
        // Cross-thread login replies (`Cx::post_action`-delivered).
        for action in actions {
            let Some(la) = action.downcast_ref::<LoginAsyncAction>() else {
                continue;
            };
            match la.kind {
                LoginAsyncEvent::SendCodeReply => {
                    if let Some(err) = la.error.as_ref() {
                        self.login_set_status(cx, err);
                    } else {
                        self.login_set_status(cx, "Code sent — check your email.");
                        self.ui
                            .view(cx, ids!(login_email_step))
                            .set_visible(cx, false);
                        self.ui
                            .view(cx, ids!(login_code_step))
                            .set_visible(cx, true);
                        self.ui.redraw(cx);
                    }
                }
                LoginAsyncEvent::VerifyReply => {
                    if let Some(err) = la.error.as_ref() {
                        self.login_set_status(cx, err);
                    } else {
                        self.login_set_status(cx, "");
                        self.show_login(cx, false);
                        // Reset step visibility for a future logout.
                        self.ui
                            .view(cx, ids!(login_email_step))
                            .set_visible(cx, true);
                        self.ui
                            .view(cx, ids!(login_code_step))
                            .set_visible(cx, false);
                        // Pick up the fresh bearer without an app restart.
                        self.connect_transport(cx);
                        self.clear_chat(cx);
                    }
                }
                LoginAsyncEvent::SoloReply => {
                    if let Some(err) = la.error.as_ref() {
                        // Login-free flow: no OTP fallback UI — surface the
                        // reason on the shell status line and stay up.
                        self.ui.label(cx, ids!(status_label)).set_text(
                            cx,
                            &format!("Sign-in unavailable: {err}"),
                        );
                        self.ui.redraw(cx);
                    } else {
                        // Refresh cached identity from the (possibly
                        // solo-rewritten) server config before connecting.
                        if let Some(cfg) = crate::app::login::load_server_config() {
                            if let Ok(u) = url::Url::parse(&cfg.server_url) {
                                self.login_server_url = Some(u);
                            }
                            self.login_profile_id =
                                Some(ProfileId::from(cfg.profile_id));
                        }
                        // Pick up the fresh bearer without an app restart.
                        self.connect_transport(cx);
                        self.clear_chat(cx);
                    }
                }
            }
        }

        // Profile dropdown selection. M1 has at most one stub label; W08
        // populates `available_profiles` from `/api/my/profile` and switches
        // sessions when the user picks a different one. Until then we just
        // record the selection so `update_status` can reflect it.
        if let Some(index) = self
            .ui
            .drop_down(cx, ids!(backend_dropdown))
            .selected(actions)
        {
            if let Some((profile_id, _label)) = self.available_profiles.get(index) {
                self.current_profile = Some(profile_id.clone());
                self.update_status(cx);
            }
        }

        // (Per-message delete handler removed with the bubble close buttons
        // — user directive.)

        // W04 — fold `SessionListAction`s posted from REST hydrate / delete
        // tasks plus the `SessionList` widget's own click events. See
        // `app/src/app/sessions.rs`.
        for action in actions {
            // Session-resume history arrived (`session/hydrate` reply routed
            // through the transport drain). Fill the chat thread if the user
            // is still on that session.
            if let Some(h) =
                action.downcast_ref::<crate::backend::octos_ui::SessionResumeHydrated>()
            {
                if self.session_id == Some(h.session_id) {
                    let count = {
                        let mut data = CHAT_DATA.write().unwrap();
                        data.messages = h
                            .messages
                            .iter()
                            .filter_map(|(role, content)| {
                                let role = match role.as_str() {
                                    "user" => ChatRole::User,
                                    "assistant" => ChatRole::Assistant,
                                    // Tool/system rows aren't chat bubbles.
                                    _ => return None,
                                };
                                Some(ChatMessage { role, text: content.clone() })
                            })
                            .collect();
                        data.is_streaming = false;
                        data.messages.len()
                    };
                    self.update_status(cx);
                    self.update_empty_state_visibility(cx);
                    let chat_list = self.ui.widget(cx, ids!(chat_list));
                    let list = chat_list.portal_list(cx, ids!(list));
                    list.set_tail_range(true);
                    list.set_first_id_and_scroll(count.saturating_sub(1), 0.0);
                    cx.redraw_all();
                }
                continue;
            }
            let Some(sa) = action.downcast_ref::<SessionListAction>() else { continue };
            match sa {
                SessionListAction::Hydrated(list) => {
                    let mut state = APP_STATE.write().unwrap();
                    // Replace whatever skeleton was there. W04 § 4 calls
                    // `/api/sessions` "Locked"; the merged list is canonical.
                    state.sessions = octos_app_store::sessions::SessionMap::new();
                    // Insert reverse — `SessionMap::insert` puts the newest
                    // at the front, but the wire returns most-recent-first;
                    // pushing in reverse keeps the visible order stable.
                    for s in list.iter().rev() {
                        state.sessions.insert(s.clone());
                    }
                    drop(state);
                    self.ui.redraw(cx);
                }
                SessionListAction::Failed(msg) => {
                    // Surface in the status label until the M2 toast queue
                    // lands. Don't clobber the existing label if it carries
                    // an error from the chat path.
                    log::warn!("session list REST: {msg}");
                }
                SessionListAction::Selected(id) => {
                    {
                        let mut state = APP_STATE.write().unwrap();
                        // W04 / M2 — also flip out of Content (or wherever)
                        // back to Chat so picking a session in the sidebar
                        // re-shows the chat surface.
                        octos_app_store::state::reduce(
                            &mut state,
                            octos_app_store::state::Event::Navigation(
                                NavigationEvent::OpenSession(id.clone()),
                            ),
                        );
                    }
                    // Resume the server-side session and request its history
                    // (`session/hydrate` → `SessionResumeHydrated` action).
                    let resumed = self
                        .agent
                        .as_mut()
                        .and_then(|agent| agent.resume_session(cx, &id.0));
                    if let Some(sid) = resumed {
                        self.session_id = Some(sid);
                        self.current_prompt = None;
                        {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.messages.clear();
                            data.streaming_text.clear();
                            data.thinking_text.clear();
                            data.is_streaming = false;
                            data.a2app_state.clear();
                        }
                        // The resumed session may or may not carry the Splash
                        // manual in its history — re-prime on next A2App use.
                        self.splash_primed = false;
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, "Loading session\u{2026}");
                        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
                        self.update_empty_state_visibility(cx);
                        self.collapse_sidebar_if_narrow(cx);
                        cx.redraw_all();
                    }
                    self.show_screen_for_nav(cx);
                }
                SessionListAction::DeleteRequested(id) => {
                    // Optimistic remove + spawn the REST DELETE.
                    {
                        let mut state = APP_STATE.write().unwrap();
                        state.sessions.remove(id);
                        if state.current_session.as_ref() == Some(id) {
                            state.current_session = None;
                        }
                    }
                    let cfg = Self::placeholder_transport_config();
                    let rest_client = Self::build_rest_client(&cfg);
                    let fallback_profile = octos_app_store::auth::ProfileId::from(
                        cfg.profile_id.0.clone(),
                    );
                    sessions_mod::delete_session_remote(rest_client, id.clone(), fallback_profile);
                    self.ui.redraw(cx);
                }
                SessionListAction::Deleted(_id) => {
                    // Optimistic remove already applied; nothing to do until
                    // M2 toast surfaces a "deleted" confirmation.
                }
            }
        }

        // W05 — Approve / Deny clicks bubble through `ApprovalUiAction`,
        // dispatched in `app/src/app/approvals.rs::post_decision`. Optimistic
        // local transition to `PendingResponse` happens here so the buttons
        // immediately disable; the wire RPC reply lands as
        // `ApprovalAsyncAction` (see below).
        for action in actions {
            let Some(ui_a) = action.downcast_ref::<crate::app::approvals::ApprovalUiAction>()
            else {
                continue;
            };
            {
                let mut state = APP_STATE.write().unwrap();
                // `ApprovalDecision` is no longer `Copy` (FIX-01); clone for
                // both call sites below.
                state
                    .approvals
                    .pending_response(&ui_a.approval_id, ui_a.decision.clone());
            }
            if let Some(handle) = self.approval_handle.as_ref() {
                handle.respond(
                    ui_a.session_id.clone(),
                    ui_a.approval_id.clone(),
                    ui_a.decision.clone(),
                    ui_a.scope.clone(),
                );
            } else {
                // No agent yet (M1 boots without one) — surface as failed so
                // the buttons re-enable.
                let mut state = APP_STATE.write().unwrap();
                state
                    .approvals
                    .failed(&ui_a.approval_id, "agent not initialized");
            }
            self.ui.redraw(cx);
        }
        // W05 — wire RPC reply lands here. `Accepted` flips to `Decided`.
        // On `Failed` with code `-32011 APPROVAL_NOT_PENDING`, parse
        // `data.recorded_decision` and collapse the retry into the same
        // `Decided` transition the success path uses (handles double-click
        // idempotently; see octos-cli/src/api/ui_protocol_approvals.rs:198
        // and the v1 spec § approval/respond). Anything else flips to
        // `Failed { msg }` (the user can re-click; server-side idempotency
        // catches duplicates).
        const APPROVAL_NOT_PENDING: i64 = -32011;
        for action in actions {
            let Some(async_a) =
                action.downcast_ref::<crate::app::approvals::ApprovalAsyncAction>()
            else {
                continue;
            };
            let mut state = APP_STATE.write().unwrap();
            match &async_a.outcome {
                crate::app::approvals::ApprovalAsyncOutcome::Accepted { .. } => {
                    // FIX-01: ApprovalDecision is no longer Copy.
                    state
                        .approvals
                        .decided(&async_a.approval_id, async_a.decision.clone());
                }
                crate::app::approvals::ApprovalAsyncOutcome::Failed { message, code, data } => {
                    if *code == APPROVAL_NOT_PENDING {
                        let recorded = data
                            .as_ref()
                            .and_then(|d| d.get("recorded_decision"))
                            .and_then(|d| d.as_str())
                            .and_then(parse_recorded_decision)
                            .unwrap_or_else(|| async_a.decision.clone());
                        state.approvals.decided(&async_a.approval_id, recorded);
                    } else {
                        state.approvals.failed(&async_a.approval_id, message.clone());
                    }
                }
            }
            drop(state);
            self.ui.redraw(cx);
        }

        // ---- W04 / M2 — Content nav + filter wiring ----------------------
        if self.ui.button(cx, ids!(nav_content)).clicked(actions) {
            self.navigate_to_content(cx);
            self.collapse_sidebar_if_narrow(cx);
        }

        // (Coding / Studio / Slides / Sites navs removed — unsupported in
        // this build.)

        // ---- W07 / M3 — ProducerUiAction (source add / open external) -
        for action in actions {
            let Some(pa) =
                action.downcast_ref::<crate::app::producers::ProducerUiAction>()
            else {
                continue;
            };
            match pa {
                crate::app::producers::ProducerUiAction::AddSource { kind, text } => {
                    crate::app::producers::fold_add_source(*kind, text.clone());
                    self.ui.redraw(cx);
                }
                crate::app::producers::ProducerUiAction::SourceInputChanged {
                    kind,
                    text,
                } => {
                    crate::app::producers::fold_source_input_changed(
                        *kind,
                        text.clone(),
                    );
                }
                crate::app::producers::ProducerUiAction::OpenGeneration {
                    kind: _,
                    url,
                } => {
                    crate::app::producers::open_generation_externally(url);
                }
            }
        }

        // ---- W06 / M3 — CodingUiAction (queue / history selection) -------
        for action in actions {
            let Some(ca) = action.downcast_ref::<crate::app::coding::CodingUiAction>()
            else {
                continue;
            };
            match ca {
                crate::app::coding::CodingUiAction::SelectApproval(id) => {
                    crate::app::coding::fold_select_approval(id.clone());
                    self.ui.redraw(cx);
                }
                crate::app::coding::CodingUiAction::SelectHistory(id) => {
                    // History click reuses the same selection slot; the
                    // right-pane preview stays read-only because the
                    // `ApprovalState::Decided` rows have no Approve/Deny
                    // controls in the queue card.
                    crate::app::coding::fold_select_approval(id.clone());
                    self.ui.redraw(cx);
                }
                crate::app::coding::CodingUiAction::SelectTask(task_id) => {
                    crate::app::coding::fold_select_task(task_id.clone());
                    self.fire_task_output_read(task_id.clone());
                    self.ui.redraw(cx);
                }
            }
        }

        // ---- W06 / M3 — TaskOutputAction (output buffer fold) ------------
        for action in actions {
            let Some(ta) = action.downcast_ref::<crate::app::coding::TaskOutputAction>()
            else {
                continue;
            };
            match &ta.outcome {
                crate::app::coding::TaskOutputOutcome::Loaded(_) => {
                    // Clone the action so `fold_task_output` can take
                    // ownership — `downcast_ref` returns a borrow.
                    let cloned = crate::app::coding::TaskOutputAction {
                        task_id: ta.task_id.clone(),
                        session_id: ta.session_id.clone(),
                        outcome: match &ta.outcome {
                            crate::app::coding::TaskOutputOutcome::Loaded(r) => {
                                crate::app::coding::TaskOutputOutcome::Loaded(r.clone())
                            }
                            crate::app::coding::TaskOutputOutcome::Failed(s) => {
                                crate::app::coding::TaskOutputOutcome::Failed(s.clone())
                            }
                        },
                    };
                    crate::app::coding::fold_task_output(cloned);
                    self.ui.redraw(cx);
                }
                crate::app::coding::TaskOutputOutcome::Failed(msg) => {
                    log::warn!("task/output/read: {msg}");
                }
            }
        }
        if self
            .ui
            .button(cx, ids!(content_refresh_button))
            .clicked(actions)
        {
            self.fire_content_hydrate();
        }
        if let Some(idx) = self
            .ui
            .drop_down(cx, ids!(content_filter_dropdown))
            .selected(actions)
        {
            if let Ok(mut cs) = CONTENT_STATE.write() {
                cs.filter = ContentFilter::from_dropdown_index(idx);
            }
            self.fire_content_hydrate();
            self.ui.redraw(cx);
        }
        if let Some(text) = self
            .ui
            .text_input(cx, ids!(content_search_input))
            .changed(actions)
        {
            if let Ok(mut cs) = CONTENT_STATE.write() {
                cs.search = text;
            }
            self.ui.redraw(cx);
        }

        // ---- W04 / M2 — ContentAction (REST hydrate + card click) -------
        for action in actions {
            let Some(ca) = action.downcast_ref::<ContentAction>() else { continue };
            match ca {
                ContentAction::Hydrated(metas) => {
                    let mut state = APP_STATE.write().unwrap();
                    content_mod::fold_hydrated(&mut state, metas.clone());
                    drop(state);
                    if let Ok(mut cs) = CONTENT_STATE.write() {
                        cs.last_error = None;
                    }
                    self.ui.redraw(cx);
                }
                ContentAction::Failed(msg) => {
                    log::warn!("content hydrate REST: {msg}");
                    if let Ok(mut cs) = CONTENT_STATE.write() {
                        cs.last_error = Some(msg.clone());
                    }
                    self.ui.redraw(cx);
                }
                ContentAction::Open(handle) => {
                    self.open_viewer_for(cx, handle.clone());
                }
            }
        }

        // ---- W04 / M2 — ViewerAction (overlay close, prev/next, OS handoff) -
        for action in actions {
            let Some(va) = action.downcast_ref::<ViewerAction>() else { continue };
            match va {
                ViewerAction::Close => self.close_viewer(cx),
                ViewerAction::AlbumStep(delta) => self.album_step(cx, *delta),
                ViewerAction::OpenInOs(handle) => self.open_in_os(handle),
                ViewerAction::MarkdownLoaded { handle, body } => {
                    if let Ok(mut vs) = VIEWER_STATE.write() {
                        vs.markdown_cache.insert(handle.clone(), body.clone());
                        vs.last_error = None;
                    }
                    self.ui.redraw(cx);
                }
                ViewerAction::MarkdownFailed { handle, error } => {
                    log::warn!("markdown fetch {handle}: {error}");
                    if let Ok(mut vs) = VIEWER_STATE.write() {
                        vs.last_error = Some(error.clone());
                    }
                    self.ui.redraw(cx);
                }
            }
        }
    }

    fn handle_startup(&mut self, cx: &mut Cx) {
        // Android: route the real `log` facade (transport/store crates) to
        // logcat — without this their records are dropped silently.
        octos_app_transport::install_android_logger();

        // This app is a full-screen A2App card generator: A2App mode is always
        // on (the toggle was removed), and the floating composer starts expanded.
        self.splash_mode = true;
        self.composer_shown = true;

        // Android: the process has no usable HOME, and everything below
        // (server.json, the token store, chat persistence) is HOME-relative.
        // Point HOME at the app-private files dir makepad reports from
        // `getFilesDir()` before any config path is resolved.
        #[cfg(target_os = "android")]
        if let Some(dir) = cx.get_data_dir() {
            std::env::set_var("HOME", &dir);
        }

        // No-UI provisioning: a `makepad.APP_CONFIG` launch-intent extra
        // (`adb shell am start … --es makepad.APP_CONFIG
        // 'http://host:port|profile|token'`) surfaces here as the
        // MAKEPAD_APP_CONFIG env var. It writes the server config + bearer
        // BEFORE the boot-auth decision, so a provisioned device lands
        // straight on the home shell — no LoginScreen typing. A QR-scan
        // onboarding can feed the same `apply_provision_string` entry later.
        if let Ok(prov) = std::env::var("MAKEPAD_APP_CONFIG") {
            match crate::app::login::apply_provision_string(&prov) {
                Ok(()) => log::info!("provisioned from launch intent"),
                Err(e) => log::warn!("provisioning failed: {e}"),
            }
        }

        // Construct the OctosUiAgent up-front so the chat surface has
        // somewhere to send a prompt (config/token state as currently on
        // disk; re-run by the login flow once a fresh bearer lands).
        self.connect_transport(cx);

        // Profile dropdown. W08 will populate `available_profiles` from
        // `/api/my/profile`; for M1 we hand the dropdown the stub label
        // already declared in the live-DSL.
        if !self.available_profiles.is_empty() {
            self.ui
                .drop_down(cx, ids!(backend_dropdown))
                .set_selected_item(cx, 0);
            self.current_profile = self
                .available_profiles
                .first()
                .map(|(id, _)| id.clone());
        }

        self.update_status(cx);
        self.update_connection_indicator(cx);
        self.update_context_indicator(cx);
        self.update_empty_state_visibility(cx);
        self.ui
            .slider(cx, ids!(opacity_slider))
            .set_value(cx, DEFAULT_GLASS_OPACITY);
        // Thinking toggle is inert in M1 (see `handle_actions` comment); the
        // initial state is whatever the DSL declared (`active: false`).
        self.apply_glass_opacity(cx, DEFAULT_GLASS_OPACITY);

        // ---- W08 — boot decision: LoginScreen vs Home ---------------------
        // Login-free boot (user directive): the LoginScreen is never shown.
        // Auth resolves silently — stored bearer > background solo sign-in
        // against the configured (or default on-device) server. Provisioning
        // stays available via the `makepad.APP_CONFIG` intent extra.
        let authed = self.boot_is_authed();
        self.show_login(cx, false);
        // W04 / M2 — make sure the chat_screen / content_screen pair
        // matches the boot navigation state (defaults to Home → Chat).
        self.show_screen_for_nav(cx);
        if authed {
            // Open the first session immediately so the composer is live.
            self.clear_chat(cx);
        } else {
            self.auto_solo_login(cx);
        }
        // Phone boot: land on the chat surface, not the menu — ☰ opens it.
        self.collapse_sidebar_if_narrow(cx);
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        // NOTE: `agent.notify(...)` for A2App/Splash button callbacks is
        // registered inside `makepad_widgets::script_mod` so it reaches the
        // isolated Splash VMs too (see aichat/widgets/src/lib.rs).
        crate::makepad_widgets::script_mod(vm);
        crate::makepad_code_editor::script_mod(vm);
        crate::makepad_diagram_kit::script_mod(vm);
        // W08 — register the LoginScreen DSL prototype before this file's
        // `script_mod` runs so `body +: { LoginScreen { … } }` resolves.
        crate::app::login::script_mod(vm);
        // W05 — register the ApprovalsPane / ApprovalCardView prototypes
        // so the chat scene can place `ApprovalsPane {}` between
        // `chat_shell` and `composer_row`.
        crate::app::approvals::script_mod(vm);
        // W04 / M2 — register `ContentBrowser` and `ViewerOverlay`
        // prototypes so the live-DSL `content_screen := ContentBrowser {}`
        // and `viewer_overlay := ViewerOverlay {}` references resolve.
        crate::app::content_browser::script_mod(vm);
        crate::app::viewers::script_mod(vm);
        // Swimming-octopus thinking indicator (chat screen, above composer).
        crate::app::octo_thinking::script_mod(vm);
        // W06 / M3 — register `CodingScreen` so the live-DSL
        // `coding_screen := CodingScreen {}` sibling resolves.
        crate::app::coding::script_mod(vm);
        // W07 / M3 — `StudioScreen` / `SlidesScreen` / `SitesScreen`
        // and the inner `GenerationCard` DSL prototypes are inlined into
        // `self::script_mod` below (mirrors the `SessionList` / `TaskDock`
        // pattern); their Rust impls live in `app/src/app/producers.rs`.
        self::script_mod(vm)
    }

    fn after_new_from_script(_vm: &mut ScriptVm, app: &mut Self) {
        // W04 will replace this with a SQLite per-session cache hydrate +
        // REST snapshot. For now, `load_from_disk` is a no-op stub so the
        // binary boots without touching disk.
        CHAT_DATA.write().unwrap().messages = ChatData::load_from_disk();
        // `available_profiles` stays empty until W08 hydrates it.
        app.available_profiles = Vec::new();
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Streaming repaint tick — see `stream_tick` field docs.
        if self.stream_tick.is_event(event).is_some() {
            if self.stream_dirty {
                self.stream_dirty = false;
                cx.redraw_all();
            } else if !CHAT_DATA.read().map(|d| d.is_streaming).unwrap_or(false) {
                // Turn finished and nothing pending — park the interval.
                cx.stop_timer(self.stream_tick);
                self.stream_tick = Timer::empty();
            }
        }
        // Toast auto-dismiss: pop the shown toast and advance to the next.
        if self.toast_timer.is_event(event).is_some() {
            self.toast_timer = Timer::empty();
            if let Ok(mut state) = APP_STATE.write() {
                octos_app_store::state::reduce(
                    &mut state,
                    octos_app_store::state::Event::DismissOldestToast,
                );
            }
            self.sync_toasts(cx);
        }
        // Android: window size may be unknown during handle_startup, so
        // re-apply the phone-boot sidebar collapse once the first real
        // layout exists.
        if let Event::Draw(_) = event {
            static FIRST_DRAW: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(true);
            if FIRST_DRAW.swap(false, std::sync::atomic::Ordering::Relaxed) {
                self.collapse_sidebar_if_narrow(cx);
            }
        }
        if let Event::WindowDragQuery(dq) = event {
            if Some(dq.window_id) == self.ui.window(cx, ids!(main_window)).window_id() {
                let size = self.ui.window(cx, ids!(main_window)).get_inner_size(cx);
                if should_start_window_drag(dq.abs, size) {
                    dq.response.set(WindowDragQueryResponse::Caption);
                    cx.set_cursor(MouseCursor::Default);
                }
            }
        }

        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());

        // Transport wake-ups arrive as signals; refresh the top-bar
        // connection dot/label from APP_STATE so Live/Reconnecting/Offline
        // tracks reality instead of the boot snapshot.
        if let Event::Signal = event {
            self.update_connection_indicator(cx);
        self.update_context_indicator(cx);
            // Streaming state flips on transport events — keep the octopus
            // (and empty-state) in sync even when no widget action fired.
            self.update_empty_state_visibility(cx);
            // Re-assert the Profile pill: a set_labels issued during
            // handle_startup can land on a not-yet-ready widget ref and
            // silently no-op, leaving the "(no profile)" stub on screen.
            if let Some((_, label)) = self.available_profiles.first() {
                let dd = self.ui.drop_down(cx, ids!(backend_dropdown));
                if &dd.selected_label() != label {
                    dd.set_labels(cx, vec![label.clone()]);
                    dd.set_selected_item(cx, 0);
                    dd.redraw(cx);
                }
            }
        }

        if let Some(agent) = &mut self.agent {
            for event in agent.handle_event(cx, event) {
                match event {
                    AgentEvent::SessionReady { .. } => {
                        self.update_status(cx);
                    }
                    AgentEvent::SessionError { error, .. } => {
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, &format!("Error: {}", error));
                    }
                    AgentEvent::TextDelta { text, .. } => {
                        // Perf: tokens arrive far faster than 60 fps, and the
                        // draw path re-parses the whole accumulated reply —
                        // so only accumulate here and let the ~10 Hz
                        // `stream_tick` drive redraws (first delta of a burst
                        // paints immediately).
                        {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.streaming_text.push_str(&text);
                        }
                        self.stream_dirty = true;
                        if self.stream_tick.is_empty() {
                            self.stream_tick = cx.start_interval(0.1);
                            self.stream_dirty = false;
                            cx.redraw_all();
                        }
                    }
                    AgentEvent::ThinkingDelta { text, .. } => {
                        let first = {
                            let mut data = CHAT_DATA.write().unwrap();
                            let first = data.thinking_text.is_empty();
                            data.thinking_text.push_str(&text);
                            first
                        };
                        if first {
                            self.ui
                                .label(cx, ids!(status_label))
                                .set_text(cx, "Thinking...");
                        }
                        self.stream_dirty = true;
                        if self.stream_tick.is_empty() {
                            self.stream_tick = cx.start_interval(0.1);
                            self.stream_dirty = false;
                            cx.redraw_all();
                        }
                    }
                    AgentEvent::TurnComplete { .. } => {
                        let mut data = CHAT_DATA.write().unwrap();
                        let text = std::mem::take(&mut data.streaming_text);
                        log!(
                            "aichat UI turn complete content_chars={}",
                            text.chars().count()
                        );
                        data.thinking_text.clear();
                        let mut rendered_card = false;
                        if !text.is_empty() {
                            if assistant_message_is_safe_to_store(&text) {
                                // Persist a named A2App card so it can be
                                // retrieved by name and refined over time.
                                if let Some(body) = extract_runsplash_body(&text) {
                                    rendered_card = true;
                                    match extract_card_name(body) {
                                        Some(name) => save_a2app_card(&name, body),
                                        None => log::warn!(
                                            "a2app: runsplash card has no `// name:` line — not saved"
                                        ),
                                    }
                                }
                                data.messages.push(ChatMessage {
                                    role: ChatRole::Assistant,
                                    text,
                                });
                            } else {
                                self.ui.label(cx, ids!(status_label)).set_text(
                                    cx,
                                    "Error: incomplete diagram response discarded; retry",
                                );
                            }
                        }
                        data.is_streaming = false;
                        data.save_to_disk();
                        drop(data);

                        self.current_prompt = None;
                        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
                        self.update_empty_state_visibility(cx);
                        // A card just rendered — collapse the floating composer to
                        // the reveal pill so the card gets the full screen.
                        if rendered_card {
                            self.composer_shown = false;
                        }
                        self.sync_composer(cx);
                        // Clear the transient "Thinking..." status back to the
                        // idle connection line (it was set by ThinkingDelta and
                        // otherwise stuck after the reply landed).
                        self.update_status(cx);
                        cx.redraw_all();
                        // A full-screen card just rendered: scroll it into view
                        // (the redraw_all above can reset the list to the top).
                        if rendered_card {
                            let count = { CHAT_DATA.read().unwrap().messages.len() };
                            let list = self
                                .ui
                                .widget(cx, ids!(chat_list))
                                .portal_list(cx, ids!(list));
                            list.set_tail_range(true);
                            list.set_first_id_and_scroll(count.saturating_sub(1), 0.0);
                        }
                    }
                    AgentEvent::PromptError { error, .. } => {
                        log!("aichat UI prompt error: {}", error);
                        {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.messages.push(ChatMessage {
                                role: ChatRole::Assistant,
                                text: format!("Error: {error}"),
                            });
                            data.is_streaming = false;
                            data.thinking_text.clear();
                            data.save_to_disk();
                        }
                        self.current_prompt = None;
                        self.ui.view(cx, ids!(cancel_button)).set_visible(cx, false);
                        self.update_empty_state_visibility(cx);
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, &format!("Error: {}", error));
                        cx.redraw_all();
                    }
                    AgentEvent::ToolRequest { .. } => {}
                }
            }
        }

        // W04 follow-up #3 — refresh the top-bar connection indicator each
        // tick. `OctosUiAgent` mirrors transport `ConnectionState` into
        // `APP_STATE.connection`; reading it here keeps the dot in sync
        // without a separate signal/post_action.
        self.update_connection_indicator(cx);
        self.update_context_indicator(cx);
        // Show any toasts queued by the store during this drain (compaction,
        // memory-saved, warnings).
        self.sync_toasts(cx);
        // Keep the swimming-octopus row in lockstep with `is_streaming`
        // (flips inside the agent drain above — actions, not signals).
        self.update_empty_state_visibility(cx);
    }
}

#[cfg(test)]
mod tests {
    use makepad_widgets::DVec2;

    use super::{
        assistant_message_is_safe_for_history, assistant_message_is_safe_to_store,
        glass_opacity_values, should_start_window_drag, DEFAULT_GLASS_OPACITY,
        MAX_GLASS_OPACITY, MIN_GLASS_OPACITY,
    };

    #[test]
    fn aichat_glass_opacity_slider_contract() {
        // v2: slider is a position value; per-layer alpha is derived.
        assert!((DEFAULT_GLASS_OPACITY - 0.90).abs() < f64::EPSILON);
        let values = glass_opacity_values(DEFAULT_GLASS_OPACITY);
        // Layer stack must read shell < main < sidebar < composer
        // so the wallpaper shows through more on the outer frame than on
        // the inner panels.
        assert!(values.app < values.main);
        assert!(values.main < values.sidebar);
        assert!(values.sidebar < values.composer);
        // Default keeps the wallpaper visible, but is opaque enough for text.
        assert!((0.82..0.87).contains(&values.app));
    }

    #[test]
    fn aichat_liquid_glass_shell_contract() {
        // v2: layer-stack ordering must hold at every legal slider value,
        // and no layer reaches alpha 1.0 at any slider <= 1.0.
        let low = glass_opacity_values(0.0);
        let high = glass_opacity_values(2.0);
        // Slider is clamped: low.app uses MIN_GLASS_OPACITY, high.app uses MAX.
        assert!(low.app < high.app);
        assert!(high.app > 0.90);
        assert!(high.app <= 1.0);
        // Ordering preserved across the range.
        for &slider in &[
            MIN_GLASS_OPACITY,
            0.30_f64,
            0.60,
            DEFAULT_GLASS_OPACITY,
            MAX_GLASS_OPACITY,
        ] {
            let v = glass_opacity_values(slider);
            assert!(v.app < v.main, "slider={}", slider);
            assert!(v.main <= v.sidebar, "slider={}", slider);
            assert!(v.sidebar <= v.composer, "slider={}", slider);
        }
    }

    #[test]
    fn aichat_drag_strip_preserves_resize_edges() {
        let size = DVec2 { x: 900.0, y: 700.0 };
        assert!(should_start_window_drag(
            DVec2 { x: 120.0, y: 24.0 },
            size
        ));
        assert!(!should_start_window_drag(DVec2 { x: 4.0, y: 24.0 }, size));
        assert!(!should_start_window_drag(DVec2 { x: 120.0, y: 4.0 }, size));
        assert!(!should_start_window_drag(
            DVec2 { x: 880.0, y: 24.0 },
            size
        ));
        assert!(!should_start_window_drag(
            DVec2 { x: 700.0, y: 24.0 },
            size
        ));
    }

    // (W02 strip) — `aichat_backend_type_includes_claude_code`,
    // `aichat_create_claude_code_agent`, `aichat_defaults_to_moonshot_when_available`,
    // `non_splash_prompt_documents_sequence_diagrams`, and
    // `non_splash_prompt_documents_all_diagram_types` lived here. They tested
    // `BackendType` and the inline `system_prompt`, both of which are gone.

    #[test]
    fn history_injection_allows_valid_diagram_assistant_messages() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft","kind":"start"},{"id":"done","label":"Done","kind":"end","role":"focal"}],"transitions":[{"from":"draft","to":"done","label":"submit"}]}
```"#;

        assert!(assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn history_injection_rejects_incomplete_diagram_assistant_messages() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft","kind":"start"},{"id":"pending","label":"Pending Payment"},{"id":"paid","label":"
"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn history_injection_rejects_invalid_closed_diagram_assistant_messages() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft"}],
```"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn history_injection_allows_non_diagram_assistant_messages() {
        let text = "这里是普通解释，没有 diagram fence。";

        assert!(assistant_message_is_safe_to_store(text));
        assert!(assistant_message_is_safe_for_history(text));
    }

    // Regression: an unclosed *non-diagram* fence (e.g. response truncated
    // mid `rust`/`mermaid` block) was discarding the entire reply because
    // FenceScanError::Unclosed was treated the same as a malformed diagram.
    #[test]
    fn store_keeps_reply_with_unclosed_non_diagram_fence() {
        let text = "Here's a markdown demo:\n\n```rust\nfn main() {\n    println!(\"hi\";\n";
        assert!(assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn store_rejects_bad_diagram_even_with_later_unclosed_non_diagram_fence() {
        let text = r#"```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft"}],
```

```rust
fn main() {
"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn outer_markdown_wrapper_is_unwrapped_before_diagram_safety_scan() {
        let text = r#"```markdown
Here is a diagram:

```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft","kind":"start"},{"id":"done","label":"Done","kind":"end"}],"transitions":[{"from":"draft","to":"done","label":"submit"}]}
```
```"#;

        assert!(assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }

    #[test]
    fn outer_markdown_wrapper_rejects_bad_inner_diagram() {
        let text = r#"```markdown
Here is a broken diagram:

```diagram
{"type":"state","orientation":"lr","states":[{"id":"draft","label":"Draft"}],
```
```"#;

        assert!(!assistant_message_is_safe_to_store(text));
        assert!(!assistant_message_is_safe_for_history(text));
    }
}
