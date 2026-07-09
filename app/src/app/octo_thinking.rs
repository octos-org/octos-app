//! Abstract "thinking" indicator — morphing 3D metaballs that flash color
//! while the model reasons/streams. Pure procedural shader, no assets.
//!
//! Four moving centers contribute to a classic metaball potential field;
//! the iso-surface is shaded like a glossy sphere (diffuse + specular from
//! an analytic field-gradient normal) and tinted by a time-cycling cosine
//! palette, so the blobs merge/split and pulse through the spectrum.
//!
//! Self-animates only while drawn: `draw_walk` arms a NextFrame, the handler
//! advances the `time` uniform and redraws. Hidden (row `visible:false`) → no
//! draw → the loop parks itself, so there's no idle cost.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*

    mod.widgets.OctoThinking = #(crate::app::octo_thinking::OctoThinking::register_widget(vm)) {
        // Full page width so blobs glide edge-to-edge instead of being
        // clipped inside a small centered box.
        width: Fill
        height: 84
        show_bg: true
        draw_bg +: {
            time: uniform(0.0)
            pixel: fn() {
                let t = self.time
                let w = self.rect_size.x
                let h = self.rect_size.y

                // Center + normalize so y ∈ [-1,1], x scaled by aspect. The
                // field lives in this space; blobs glide across the width.
                let aspect = w / h
                let p = (self.pos - vec2(0.5, 0.5)) * vec2(2.0 * aspect, 2.0)

                // Four wandering centers (phase-shifted sin/cos). Amplitudes
                // are bounded to (extent − visible radius) on each axis so the
                // FULL blob stays inside the widget and appears to bounce off
                // the borders instead of being clipped at the edges. `vr` ≈ the
                // metaball's visible radius for the radii² below.
                let vr = 0.70
                let xr = aspect - vr
                let yr = 1.0 - vr
                let c0 = vec2(sin(t * 1.3) * xr, sin(t * 1.7) * yr)
                let c1 = vec2(sin(t * 0.9 + 2.0) * xr, sin(t * 1.1 + 1.0) * yr)
                let c2 = vec2(cos(t * 1.5 + 0.5) * xr, cos(t * 1.9) * yr)
                let c3 = vec2(sin(t * 1.1 + 4.0) * xr, cos(t * 0.7 + 3.0) * yr)
                let r0 = 0.40
                let r1 = 0.36
                let r2 = 0.40
                let r3 = 0.34

                // Metaball field f and its gradient (for the surface normal),
                // each computed as a single expression — MPSL `let`s are
                // immutable, so no running accumulation. Term: rᵢ²/|p-cᵢ|²;
                // gradient term: -2 rᵢ² (p-cᵢ)/|p-cᵢ|⁴.
                let d0 = p - c0
                let q0 = dot(d0, d0) + 0.0008
                let d1 = p - c1
                let q1 = dot(d1, d1) + 0.0008
                let d2 = p - c2
                let q2 = dot(d2, d2) + 0.0008
                let d3 = p - c3
                let q3 = dot(d3, d3) + 0.0008

                let f = r0 / q0 + r1 / q1 + r2 / q2 + r3 / q3
                let grad = (0.0 - 2.0) * (
                    r0 * d0 / (q0 * q0)
                    + r1 * d1 / (q1 * q1)
                    + r2 * d2 / (q2 * q2)
                    + r3 * d3 / (q3 * q3)
                )

                // Soft iso-surface mask + a fainter outer glow halo. Wider
                // core smoothstep = rounder, less ring-y edges.
                let core = smoothstep(0.75, 1.6, f)
                let glow = smoothstep(0.30, 0.90, f)

                // Fake-3D normal: gradient points inward (toward centers), so
                // the outward surface tilt is -grad; z gives the dome bulge.
                // Gentler scale keeps the dome smooth instead of noisy.
                let n = normalize(vec3(-grad * 0.09, 1.0))
                let l = normalize(vec3(-0.4, -0.5, 0.75))
                let diff = clamp(dot(n, l), 0.0, 1.0)
                // Softer, broader Blinn-Phong highlight = one glossy sheen
                // per blob rather than sharp concentric rings.
                let hlf = normalize(l + vec3(0.0, 0.0, 1.0))
                let spec = pow(clamp(dot(n, hlf), 0.0, 1.0), 12.0) * 0.6

                // Flashing color: cosine palette driven mainly by time (a
                // smooth global cycle) with a slow spatial drift, so the whole
                // cluster sweeps the spectrum instead of banding into rings.
                let hue = t * 0.11 + p.x * 0.05 - p.y * 0.04
                let pal = vec3(0.55, 0.5, 0.55)
                    + vec3(0.45, 0.5, 0.45)
                        * cos(6.28318 * (hue + vec3(0.0, 0.33, 0.67)))
                let pulse = 0.85 + 0.15 * sin(t * 2.6)

                let lit = pal * (0.42 + 0.72 * diff) * pulse
                    + vec3(spec, spec, spec)
                // Halo picks up a dim tint of the palette so merges glow.
                let halo = pal * 0.20 * (glow - core)

                let rgb = lit * core + halo
                let alpha = clamp(core + (glow - core) * 0.35, 0.0, 1.0)
                return vec4(rgb * alpha, alpha)
            }
        }
    }
}

/// Self-animating morphing-metaball indicator. Wrap in a `View` and toggle
/// that view's visibility; the animation only runs while actually drawn.
#[derive(Script, ScriptHook, Widget)]
pub struct OctoThinking {
    #[deref]
    view: View,
    #[rust]
    next_frame: NextFrame,
}

impl Widget for OctoThinking {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Some(ev) = self.next_frame.is_event(event) {
            self.view
                .draw_bg
                .set_uniform(cx, live_id!(time), &[ev.time as f32]);
            self.view.redraw(cx);
        }
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Arm the animation while visible; parks automatically when hidden
        // (no draw → no re-arm).
        self.next_frame = cx.new_next_frame();
        self.view.draw_walk(cx, scope, walk)
    }
}
