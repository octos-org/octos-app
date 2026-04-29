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
    Capabilities, ProfileId as TransportProfileId, SecretString, TransportConfig,
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

    let SendButton = ButtonFlat {
        width: 44
        height: 44
        padding: 0
        draw_text +: {
            color: ai_ink
            text_style +: { font_size: 26 }
        }
        draw_bg +: {
            hover: instance(0.0)
            down: instance(0.0)
            focus: instance(0.0)
            disabled: instance(0.0)
            color: ai_gold
            color_hover: #xFFD98B
            border_color: #xFFF0D277
            border_size: 1.0
            border_radius: 10.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let r = min(self.rect_size.x, self.rect_size.y) * 0.5 - 3.0
                let center = self.rect_size * 0.5
                let p = self.pos - vec2(0.5, 0.5)
                let radial = clamp(length(p) * 2.0, 0.0, 1.0)
                let top_highlight = clamp(1.0 - self.pos.y * 2.2, 0.0, 1.0)
                let lower_shadow = smoothstep(0.35, 1.0, self.pos.y)
                let paper_noise = (
                    Math.random_2d(self.pos * self.rect_size * 0.42)
                    + Math.random_2d(self.pos * self.rect_size * 1.3) * 0.35
                    - 0.68
                ) * 0.045
                let fill = self.color
                    .mix(self.color_focus, self.focus)
                    .mix(self.color_hover, self.hover)
                    .mix(self.color_down, self.down)
                    .mix(self.color_disabled, self.disabled)
                let glass_fill = vec4(
                    fill.rgb
                        + vec3(0.20, 0.13, 0.04) * top_highlight
                        - vec3(0.18, 0.11, 0.04) * lower_shadow
                        - vec3(0.06, 0.04, 0.02) * radial
                        + paper_noise,
                    fill.a
                )

                sdf.circle(center.x + 0.8, center.y + 1.4, r + 1.5)
                sdf.fill(#x3A241370)

                sdf.circle(center.x, center.y, r + 1.8)
                sdf.fill_keep(#xA86F35)
                sdf.stroke(#xF6D99A88, 1.0)

                sdf.circle(center.x, center.y, r)
                sdf.fill_keep(glass_fill)
                sdf.stroke(#xF9D58AAA, 1.2)

                sdf.circle(center.x - r * 0.18, center.y - r * 0.24, r * 0.46)
                sdf.stroke(#xFFF3CF48, 0.8)
                return sdf.result
            }
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
            drag_scrolling: false
            auto_tail: true
            smooth_tail: true
            selectable: true

            User := RoundedView {
                width: Fill
                height: Fit
                margin: Inset{top: 4 bottom: 4 left: 50 right: 8}
                padding: Inset{left: 12 top: 8 right: 12 bottom: 8}
                flow: Overlay
                show_bg: true
                draw_bg +: {
                    color: #x0B2A22E6
                    radius: 12.0
                }

                selectable := Markdown {
                    width: Fill
                    height: Fit
                    selectable: true
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
                        font_size: 13.0
                    }
                    display_math := MathView {
                        font_size: 15.0
                    }
                }

                View {
                    width: Fill
                    height: Fit
                    align: Align{x: 1.0}
                    delete_button := ButtonFlat {
                        width: Fit
                        height: Fit
                        padding: Inset{top: 2 bottom: 2 left: 6 right: 6}
                        margin: Inset{top: 2 right: 2}
                        text: "x"
                        draw_text +: {
                            color: #888
                            text_style +: { font_size: 9 }
                        }
                    }
                }
            }

            Assistant := RoundedView {
                width: Fill
                height: Fit
                margin: Inset{top: 4 bottom: 4 left: 8 right: 50}
                padding: Inset{left: 12 top: 8 right: 12 bottom: 8}
                flow: Overlay
                show_bg: true
                draw_bg +: {
                    color: #x0B2A22E6
                    radius: 12.0
                }

                RubberView {
                    width: Fill
                    height: Fit
                    smoothing: 0.3

                    selectable := Markdown {
                        width: Fill
                        height: Fit
                        selectable: true
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
                            font_size: 13.0
                        }
                        display_math := MathView {
                            font_size: 15.0
                        }
                    }
                }

                View {
                    width: Fill
                    height: Fit
                    align: Align{x: 1.0}
                    copy_button := ButtonFlat {
                        width: Fit
                        height: Fit
                        padding: Inset{top: 2 bottom: 2 left: 6 right: 6}
                        margin: Inset{top: 2 right: 2}
                        text: "copy"
                        draw_text +: {
                            color: #888
                            text_style +: { font_size: 9 }
                        }
                    }
                    delete_button := ButtonFlat {
                        width: Fit
                        height: Fit
                        padding: Inset{top: 2 bottom: 2 left: 6 right: 6}
                        margin: Inset{top: 2 right: 2}
                        text: "x"
                        draw_text +: {
                            color: #888
                            text_style +: { font_size: 9 }
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

                // Title + preview wrapped in a column. The trailing row_click
                // button shrinks to fill remaining width so clicks on empty
                // space still register; the explicit row_click receives the
                // primary click. delete_button sits to its right.
                row_click := ButtonFlat {
                    width: Fill
                    height: Fit
                    align: Align{x: 0.0 y: 0.5}
                    padding: 0
                    flow: Down
                    text: ""
                    draw_text +: { color: #00000000 }
                    draw_bg +: {
                        color: #00000000
                        color_hover: #00000000
                        border_size: 0.0
                        border_radius: 0.0
                    }
                    title := Label {
                        text: ""
                        draw_text.color: #xF3E3C7
                        draw_text.text_style.font_size: 12
                    }
                    preview := Label {
                        text: ""
                        draw_text.color: #xCDBF9F88
                        draw_text.text_style.font_size: 10
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

    let StudioScreen = #(crate::app::producers::StudioScreenWidget::register_widget(vm)) {
        ..ProducerBody{}
    }
    let SlidesScreen = #(crate::app::producers::SlidesScreenWidget::register_widget(vm)) {
        ..ProducerBody{}
    }
    let SitesScreen = #(crate::app::producers::SitesScreenWidget::register_widget(vm)) {
        ..ProducerBody{}
    }

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
                        padding: Inset{left: 16 top: 16 right: 16 bottom: 16}
                        spacing: 0
                        draw_bg +: {
                            tint_color: #x0D4035
                            tint_alpha: 0.66
                            border_color: ai_cyan
                            border_alpha: 0.38
                            border_width: 1.0
                            corner_radius: 30.0
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

                        // W06 / M3 — Coding workspace nav button. Mirrors
                        // `nav_content`. Flips `APP_STATE.navigation` to
                        // `CurrentScreen::Coding` via App::handle_actions.
                        nav_coding := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "⌨  Coding"
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

                        // W07 / M3 — Studio / Slides / Sites producer nav.
                        // Each routes to its `CurrentScreen::*` variant via
                        // `App::handle_actions`. The IA matches
                        // `04-IA-AND-NAVIGATION.md` § "Top-level shell".
                        nav_studio := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "🎙  Studio"
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

                        nav_slides := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "🖼  Slides"
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

                        nav_sites := ButtonFlat {
                            width: Fill
                            height: 30
                            text: "🌐  Sites"
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
                        padding: Inset{left: 34 top: 18 right: 34 bottom: 22}
                        spacing: 12
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

                            View { width: Fill height: 1 }

                            ToolbarGlass {
                                width: 286

                                ToolbarLabel {
                                    text: "Profile"
                                    width: 76
                                }

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

                            ToolbarGlass {
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
                            flow: Down
                            spacing: 12

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

                        composer_row := View {
                            width: Fill
                            height: Fit
                            align: Align{x: 0.5 y: 0.0}

                            composer := GlassPanel {
                                width: Fill{min: 620 max: 1040}
                                height: Fit
                                new_batch: true
                                flow: Down
                                padding: Inset{left: 18 top: 14 right: 14 bottom: 12}
                                spacing: 10
                                draw_bg +: {
                                    tint_color: #x0B4035
                                    tint_alpha: 0.76
                                    border_color: ai_cyan
                                    border_alpha: 0.54
                                    border_width: 1.2
                                    corner_radius: 24.0
                                    halo_color: ai_cyan
                                    halo_strength: 0.16
                                    halo_radius: 7.0
                                    highlight_strength: 0.34
                                    highlight_band_height: 48.0
                                    chroma_strength: 0.0
                                    noise_strength: 0.004
                                }

                                input := TextInput {
                                    width: Fill
                                    height: 56
                                    empty_text: "问任何事。输入 @ 使用插件或提及文件"
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
                                    spacing: 8

                                    attach_button := IconButton { text: "+" }

                                    mention_button := IconButton { text: "@" }

                                    tools_button := IconButton { text: "⌘" }

                                    Label {
                                        text: "默认权限"
                                        draw_text.color: ai_cream_dim
                                        draw_text.text_style.font_size: 11
                                    }

                                    thinking_toggle := ToggleFlat {
                                        text: "Thinking"
                                        active: false
                                        draw_text +: {
                                            color: ai_cream_dim
                                            text_style +: { font_size: 11 }
                                        }
                                    }

                                    View { width: Fill height: 1 }

                                    cancel_button := ButtonFlat {
                                        text: "Cancel"
                                        width: 72
                                        height: 32
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

                                    clear_button := PillButton {
                                        text: "Clear"
                                        width: 78
                                        height: 36
                                        draw_bg +: {
                                            color: #x08251EC8
                                            color_hover: #x123B31EE
                                            border_color: #xEAD8B83A
                                            border_size: 1.0
                                            border_radius: 10.0
                                        }
                                    }

                                    send_button := SendButton {
                                        text: "↑"
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

                        // W06 / M3 — Coding workspace. Two-pane queue +
                        // typed-preview screen, sibling to `chat_screen`
                        // and `content_screen`. App::handle_actions
                        // toggles `set_visible` for `CurrentScreen::Coding`.
                        // Hidden by default; sidebar `nav_coding` flips it on.
                        coding_screen := CodingScreen {
                            visible: false
                        }

                        // W07 / M3 — Studio / Slides / Sites producers.
                        // Three sibling screens, structurally identical
                        // (triptych: source · chat · output), gated by
                        // `CurrentScreen::Studio/Slides/Sites`. Hidden by
                        // default; sidebar `nav_studio/slides/sites` flip
                        // them on. Each shares the same shell defined in
                        // `app/src/app/producers.rs::script_mod`.
                        studio_screen := StudioScreen {
                            visible: false
                        }
                        slides_screen := SlidesScreen {
                            visible: false
                        }
                        sites_screen := SitesScreen {
                            visible: false
                        }

                        status_label := Label {
                            width: Fill
                            height: Fit
                            text: "Initializing..."
                            margin: Inset{left: 92 right: 92 top: 0 bottom: 0}
                            draw_text.text_style.font_size: 10
                            draw_text.color: #xE2D2B9AA
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
                        let streaming_body;
                        let text: &str = if data.streaming_text.is_empty() {
                            if data.thinking_text.is_empty() {
                                "..."
                            } else {
                                "Thinking..."
                            }
                        } else {
                            let opts = SanitizeOptions {
                                trim_unclosed_fence: false,
                                ..SanitizeOptions::default()
                            };
                            // Remend keeps fenced blocks, tables and math
                            // self-consistent mid-stream so the Markdown
                            // widget doesn't re-layout a half-closed block
                            // on every token.
                            streaming_body = streaming_display_with_latex_autowrap_remend(
                                &data.streaming_text,
                                opts,
                            );
                            &streaming_body
                        };
                        let mut markdown = item_widget.markdown(cx, ids!(selectable));
                        // Unwrap outer ```markdown wrapper in streaming
                        // content: some LLMs emit the wrapper as the very
                        // first tokens, so we'd otherwise render a growing
                        // code block for the whole stream.
                        markdown.set_text(cx, unwrap_outer_markdown_fence(text));
                        if just_started {
                            markdown.reset_all_streaming_animations();
                        } else {
                            markdown.start_streaming_animation();
                        }
                        item_widget.draw_all_unscoped(cx);
                        continue;
                    }

                    if let Some(msg) = data.messages.get(item_id) {
                        let is_animating = self.animating_msg == Some(item_id);
                        let template = match msg.role {
                            ChatRole::User => id!(User),
                            ChatRole::Assistant => id!(Assistant),
                        };
                        let item_widget = list.item(cx, item_id, template);
                        let mut markdown = item_widget.markdown(cx, ids!(selectable));
                        // wrap_bare_latex wraps `\cmd{…}` with `$…$` so
                        // MathView can render them.
                        let unwrapped = unwrap_outer_markdown_fence(&msg.text);
                        let rendered = wrap_bare_latex(unwrapped);
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
    ) -> (Box<dyn Agent>, crate::backend::octos_ui::ApprovalHandle) {
        let agent = OctosUiAgent::new(transport_config);
        let handle = agent.approval_handle();
        (Box::new(agent) as Box<dyn Agent>, handle)
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
            data.save_to_disk();
        }

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
        let show_empty_state = {
            let data = CHAT_DATA.read().unwrap();
            data.messages.is_empty() && !data.is_streaming
        };
        self.ui
            .view(cx, ids!(empty_state))
            .set_visible(cx, show_empty_state);
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
        self.current_prompt = Some(agent.send_prompt(cx, session_id, &text));
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
        let is_coding = matches!(nav, CurrentScreen::Coding);
        let is_studio = matches!(nav, CurrentScreen::Studio { .. });
        let is_slides = matches!(nav, CurrentScreen::Slides { .. });
        let is_sites = matches!(nav, CurrentScreen::Sites { .. });
        // Chat is the implicit default — show it for any other screen
        // that doesn't have its own dedicated sibling here.
        let is_chat =
            !is_content && !is_coding && !is_studio && !is_slides && !is_sites;
        self.ui
            .view(cx, ids!(chat_screen))
            .set_visible(cx, is_chat);
        self.ui
            .view(cx, ids!(content_screen))
            .set_visible(cx, is_content);
        self.ui
            .view(cx, ids!(coding_screen))
            .set_visible(cx, is_coding);
        self.ui
            .view(cx, ids!(studio_screen))
            .set_visible(cx, is_studio);
        self.ui
            .view(cx, ids!(slides_screen))
            .set_visible(cx, is_slides);
        self.ui
            .view(cx, ids!(sites_screen))
            .set_visible(cx, is_sites);
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

    /// Sidebar `nav_coding` click — flip to Coding. The screen reads
    /// `APP_STATE.approvals` directly so there's no separate hydrate.
    fn navigate_to_coding(&mut self, cx: &mut Cx) {
        {
            let mut state = APP_STATE.write().unwrap();
            octos_app_store::state::reduce(
                &mut state,
                octos_app_store::state::Event::Navigation(
                    NavigationEvent::NavigateTo(CurrentScreen::Coding),
                ),
            );
        }
        self.show_screen_for_nav(cx);
    }

    /// W07 — sidebar `nav_studio/slides/sites` click. Flip to the
    /// matching producer with `project: None` (the empty-index state).
    /// Once `OpenProject` lands, replace this with a call into the
    /// project-list-driven dispatch.
    fn navigate_to_producer(&mut self, cx: &mut Cx, kind: crate::app::producers::ProducerKind) {
        let target = match kind {
            crate::app::producers::ProducerKind::Studio => {
                CurrentScreen::Studio { project: None }
            }
            crate::app::producers::ProducerKind::Slides => {
                CurrentScreen::Slides { project: None }
            }
            crate::app::producers::ProducerKind::Sites => {
                CurrentScreen::Sites { project: None }
            }
        };
        {
            let mut state = APP_STATE.write().unwrap();
            octos_app_store::state::reduce(
                &mut state,
                octos_app_store::state::Event::Navigation(
                    NavigationEvent::NavigateTo(target),
                ),
            );
        }
        self.show_screen_for_nav(cx);
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
        let _params = crate::app::coding::build_output_read_params(session_id, task_id);
        // TODO(W06.taskoutput.transport): plumb the agent's
        // `cmd_tx` into a TaskOutputHandle (mirror W05's
        // `ApprovalHandle`) so we can issue
        // `OutboundCommand::RequestTaskOutput { params, reply }` and
        // forward the reply as a `TaskOutputAction::Loaded`. For
        // now the rolling buffer fills opportunistically from the
        // `task/output/delta` stream that already lands via
        // `OctosUiAgent::translate` — see open question 3 in W06 §
        // "Open questions" ("keep delta alive when navigating
        // away?"). Empty / cold sessions render the empty state.
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
        self.ui.redraw(cx);
    }

    fn close_viewer(&self, cx: &mut Cx) {
        if let Ok(mut vs) = VIEWER_STATE.write() {
            vs.open = OpenViewer::Closed;
        }
        self.ui.redraw(cx);
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
        self.ui.redraw(cx);
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
        self.login_server_url = Some(parsed);
        self.login_profile_id = Some(ProfileId::from(pid_trimmed.to_string()));
        self.login_set_status(cx, "");
        self.ui
            .view(cx, ids!(login_server_step))
            .set_visible(cx, false);
        self.ui
            .view(cx, ids!(login_email_step))
            .set_visible(cx, true);
        self.ui.redraw(cx);
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
        let has_server = crate::app::login::load_server_config().is_some();
        self.ui
            .view(cx, ids!(login_server_step))
            .set_visible(cx, !has_server);
        self.ui
            .view(cx, ids!(login_email_step))
            .set_visible(cx, has_server);
        self.ui
            .view(cx, ids!(login_code_step))
            .set_visible(cx, false);
        self.ui
            .text_input(cx, ids!(login_email_input))
            .set_text(cx, "");
        self.ui
            .text_input(cx, ids!(login_code_input))
            .set_text(cx, "");
        self.login_set_status(cx, "");
        self.show_login(cx, true);
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

/// Discriminator for cross-thread login replies. Carrying both arms through
/// one `ActionTrait` (auto-derived from `Debug + 'static` per
/// `aichat/platform/src/action.rs:21`) keeps the `Cx::post_action`
/// boilerplate down.
#[derive(Clone, Copy, Debug)]
enum LoginAsyncEvent {
    SendCodeReply,
    VerifyReply,
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
        // Cosmetic toggle in M1 — Octos handles thinking server-side per
        // profile (see `05-AICHAT-REUSE-MAP.md` table for `thinking_toggle`),
        // so the checkbox is read but its value is intentionally inert.
        // Kept in the live-DSL for visual continuity with the lifted
        // composer; W08 may repurpose it as a per-session preference.
        let _ = self.ui.check_box(cx, ids!(thinking_toggle)).changed(actions);

        // Markdown link click — dispatch through robius-open for cross-platform
        // coverage (macOS/Linux/Windows/iOS/Android/WASM). Desktop requires a
        // modifier (Cmd on macOS, Cmd/Ctrl elsewhere) so plain clicks stay
        // available for drag-selection inside the Markdown widget; mobile &
        // web have no modifier concept, so a plain tap opens the URL.
        for action in actions {
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

        // Handle message deletion
        let chat_list = self.ui.widget(cx, ids!(chat_list));
        let list = chat_list.portal_list(cx, ids!(list));
        for (item_id, item) in list.items_with_actions(actions) {
            if item.button(cx, ids!(delete_button)).pressed(actions) {
                let mut data = CHAT_DATA.write().unwrap();
                if item_id < data.messages.len() {
                    data.messages.remove(item_id);
                    data.save_to_disk();
                }
                drop(data);
                self.ui.redraw(cx);
            }
        }

        // W04 — fold `SessionListAction`s posted from REST hydrate / delete
        // tasks plus the `SessionList` widget's own click events. See
        // `app/src/app/sessions.rs`.
        for action in actions {
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
        }

        // ---- W06 / M3 — Coding nav button --------------------------------
        if self.ui.button(cx, ids!(nav_coding)).clicked(actions) {
            self.navigate_to_coding(cx);
        }

        // ---- W07 / M3 — Studio / Slides / Sites nav buttons -----------
        if self.ui.button(cx, ids!(nav_studio)).clicked(actions) {
            self.navigate_to_producer(cx, crate::app::producers::ProducerKind::Studio);
        }
        if self.ui.button(cx, ids!(nav_slides)).clicked(actions) {
            self.navigate_to_producer(cx, crate::app::producers::ProducerKind::Slides);
        }
        if self.ui.button(cx, ids!(nav_sites)).clicked(actions) {
            self.navigate_to_producer(cx, crate::app::producers::ProducerKind::Sites);
        }

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
        // Construct the OctosUiAgent up-front so the chat surface has
        // somewhere to send a prompt. The transport is lazy: `create_session`
        // is `todo!()` until W01 wires it, so the user-visible state stays
        // "Initializing..." until we plug in the real wire calls. M1 ships
        // without a real session — the empty-state stays on screen.
        let transport_config = Self::placeholder_transport_config();
        // W04 — kick off the REST session hydrate before the agent steals
        // the config. Empty bearer means we expect a 401; the failure path
        // is silent in M1 (toast queue lands in M2). When `OCTOS_BEARER` is
        // set in the environment, the list populates within `~ 500 ms p95`
        // per W04 § 12.
        log::info!(
            "boot transport: base_url={} profile_id={}",
            transport_config.base_url, transport_config.profile_id.0
        );
        let rest_client = Self::build_rest_client(&transport_config);
        let fallback_profile = octos_app_store::auth::ProfileId::from(
            transport_config.profile_id.0.clone(),
        );
        // W04 follow-up #5 — fire `/api/version` on boot, log
        // version + service, warn on a mismatched server. Off-thread so we
        // don't stall `handle_startup`.
        Self::probe_version(Self::build_rest_client(&transport_config));
        sessions_mod::hydrate_sessions(rest_client, fallback_profile);
        let (agent, approval_handle) = Self::create_octos_agent(transport_config);
        self.agent = Some(agent);
        self.approval_handle = Some(approval_handle);

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
        self.update_empty_state_visibility(cx);
        self.ui
            .slider(cx, ids!(opacity_slider))
            .set_value(cx, DEFAULT_GLASS_OPACITY);
        // Thinking toggle is inert in M1 (see `handle_actions` comment); the
        // initial state is whatever the DSL declared (`active: false`).
        self.apply_glass_opacity(cx, DEFAULT_GLASS_OPACITY);

        // ---- W08 — boot decision: LoginScreen vs Home ---------------------
        let authed = self.boot_is_authed();
        // Step 1 hides itself once a server has been configured (the
        // `boot_is_authed` side-effect set `login_server_url`); Step 2 is
        // the natural entry point on a re-launch with no token.
        let has_server = self.login_server_url.is_some();
        self.ui
            .view(cx, ids!(login_server_step))
            .set_visible(cx, !has_server);
        self.ui
            .view(cx, ids!(login_email_step))
            .set_visible(cx, has_server);
        self.ui
            .view(cx, ids!(login_code_step))
            .set_visible(cx, false);
        self.show_login(cx, !authed);
        // W04 / M2 — make sure the chat_screen / content_screen pair
        // matches the boot navigation state (defaults to Home → Chat).
        self.show_screen_for_nav(cx);
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
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
                        log!("aichat UI text delta chars={}", text.chars().count());
                        let item_id = {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.streaming_text.push_str(&text);
                            data.messages.len()
                        };
                        let chat_list = self.ui.widget(cx, ids!(chat_list));
                        let list = chat_list.portal_list(cx, ids!(list));
                        if let Some((_, item)) = list.get_item(item_id) {
                            item.widget(cx, ids!(splash_view)).redraw(cx);
                        }
                        cx.redraw_all();
                    }
                    AgentEvent::ThinkingDelta { text, .. } => {
                        log!("aichat UI thinking delta chars={}", text.chars().count());
                        {
                            let mut data = CHAT_DATA.write().unwrap();
                            data.thinking_text.push_str(&text);
                        }
                        self.ui
                            .label(cx, ids!(status_label))
                            .set_text(cx, "Thinking...");
                        cx.redraw_all();
                    }
                    AgentEvent::TurnComplete { .. } => {
                        let mut data = CHAT_DATA.write().unwrap();
                        let text = std::mem::take(&mut data.streaming_text);
                        log!(
                            "aichat UI turn complete content_chars={}",
                            text.chars().count()
                        );
                        data.thinking_text.clear();
                        if !text.is_empty() {
                            if assistant_message_is_safe_to_store(&text) {
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
                        cx.redraw_all();
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
