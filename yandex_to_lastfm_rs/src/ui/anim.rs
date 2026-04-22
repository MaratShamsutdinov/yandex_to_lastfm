use crate::config::{COVER_ANIM_MS, POPUP_FADE_IN_MS, POPUP_FADE_OUT_MS, POPUP_VISIBLE_SECS};

use super::state::PopupWindowState;

pub fn current_anim_values(state: &PopupWindowState) -> (f32, f32, f32, bool) {
    let elapsed_ms = state.shown_at.elapsed().as_millis() as u64;
    let total_ms = POPUP_VISIBLE_SECS * 1000;

    if elapsed_ms >= total_ms {
        return (0.0, 1.0, 1.0, true);
    }

    let fade_in_t = (elapsed_ms as f32 / POPUP_FADE_IN_MS as f32).clamp(0.0, 1.0);
    let fade_in_curve = hyperbolic_appear(fade_in_t, 0.1);

    let remaining_ms = total_ms.saturating_sub(elapsed_ms);
    let fade_out_t = if remaining_ms < POPUP_FADE_OUT_MS {
        let t = (remaining_ms as f32 / POPUP_FADE_OUT_MS as f32).clamp(0.0, 1.0);
        hyperbolic_reverse(t, 0.5)
    } else {
        1.0
    };

    let alpha_t = (fade_in_curve * fade_out_t).clamp(0.0, 1.0);
    let slide_t = fade_in_curve.clamp(0.0, 1.0);

    let cover_linear_t = (elapsed_ms as f32 / COVER_ANIM_MS as f32).clamp(0.0, 1.0);
    let cover_t = hyperbolic_appear(cover_linear_t, 0.05).clamp(0.0, 1.0);

    (alpha_t, slide_t, cover_t, false)
}

pub fn hyperbolic_reverse(t: f32, k: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - ((1.0 - t) / ((1.0 - t) + k * t))
}

pub fn hyperbolic_appear(t: f32, k: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    ((1.0 + k) * t) / (1.0 + k * t)
}
