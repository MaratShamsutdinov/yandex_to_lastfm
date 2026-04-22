use crate::config::{
    ARTIST_FONT_SIZE, ARTIST_TRACK_SPACING, BG_ALPHA_MAX, BG_BASE_B, BG_BASE_G, BG_BASE_R,
    CONTENT_GAP_X, COVER_CORNER_RADIUS, COVER_RISE_PX, COVER_SIZE_PX, DOMINANT_BLEND_T,
    FALLBACK_NOTE_ALPHA_MAX, FALLBACK_NOTE_FONT_SIZE, FALLBACK_NOTE_RGB, FALLBACK_NOTE_TEXT,
    FOOTER_ALPHA_MAX, FOOTER_FONT_SIZE, FOOTER_RGB, POPUP_CORNER_RADIUS, POPUP_HEIGHT,
    POPUP_INNER_MARGIN, POPUP_WIDTH, SHADOW_ALPHA_MAX, SHADOW_BLUR, SHADOW_OFFSET_X,
    SHADOW_OFFSET_Y, TEXT_MAIN_ALPHA_MAX, TEXT_MAIN_RGB, TEXT_SUB_ALPHA_MAX, TEXT_SUB_RGB,
    TEXT_TOP_OFFSET, TRACK_FONT_SIZE, TRACK_FOOTER_SPACING,
};

use super::state::PopupWindowState;
use super::text::{
    draw_text_centered, draw_text_line, draw_text_wrapped_clipped, truncate_text_to_width,
};

pub const SHADOW_PAD: i32 = 18;
pub const BORDER_PAD: i32 = 1;

pub const WINDOW_W: i32 = POPUP_WIDTH as i32 + SHADOW_PAD * 2;
pub const WINDOW_H: i32 = POPUP_HEIGHT as i32 + SHADOW_PAD * 2;

pub const ICON_BOX_WIDTH: i32 = COVER_SIZE_PX as i32;
pub const ICON_BOX_HEIGHT: i32 = COVER_SIZE_PX as i32;
pub const COVER_GAP_X: i32 = CONTENT_GAP_X as i32;

#[derive(Clone, Copy)]
pub struct RectI {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub fn raster_popup_frame(
    frame: &mut [u8],
    w: i32,
    h: i32,
    state: &PopupWindowState,
    alpha_t: f32,
    cover_t: f32,
) {
    for px in frame.chunks_exact_mut(4) {
        px[0] = 0;
        px[1] = 0;
        px[2] = 0;
        px[3] = 0;
    }

    let popup_rect = RectI {
        x: SHADOW_PAD,
        y: SHADOW_PAD,
        w: POPUP_WIDTH as i32,
        h: POPUP_HEIGHT as i32,
    };

    draw_shadow(frame, w, h, popup_rect, alpha_t);

    let bg_rgb = blended_bg_rgb(state.payload.dominant_rgb);
    let bg_alpha = (BG_ALPHA_MAX * alpha_t).round() as u8;
    draw_rounded_rect(
        frame,
        w,
        h,
        popup_rect,
        POPUP_CORNER_RADIUS as i32,
        bg_rgb,
        bg_alpha,
    );

    let border_rgb = blend_rgb(bg_rgb, [255, 255, 255], 0.08);
    draw_rounded_border(
        frame,
        w,
        h,
        popup_rect,
        POPUP_CORNER_RADIUS as i32,
        BORDER_PAD,
        border_rgb,
        (52.0 * alpha_t).round() as u8,
        bg_rgb,
        bg_alpha,
    );

    let margin = SHADOW_PAD + BORDER_PAD + POPUP_INNER_MARGIN as i32;
    let cover_offset_y = ((1.0 - cover_t) * COVER_RISE_PX).round() as i32;

    let cover_x = margin;
    let cover_y = margin + cover_offset_y;

    if let Some(ref cover) = state.cover_rgba {
        draw_cover_rgba(
            frame,
            w,
            h,
            cover,
            state.cover_width,
            state.cover_height,
            cover_x,
            cover_y,
            ICON_BOX_WIDTH,
            ICON_BOX_HEIGHT,
            COVER_CORNER_RADIUS as i32,
            cover_t * alpha_t * 0.60,
        );
    }
}

pub fn raster_text_layers(
    frame: &mut [u8],
    frame_w: i32,
    _frame_h: i32,
    state: &PopupWindowState,
    alpha_t: f32,
) {
    let popup_left = SHADOW_PAD;
    let popup_top = SHADOW_PAD;
    let popup_right = popup_left + POPUP_WIDTH as i32;

    let inner_left = popup_left + BORDER_PAD;
    let inner_right = popup_right - BORDER_PAD;

    let margin = inner_left + POPUP_INNER_MARGIN as i32;
    let cover_left = margin;
    let cover_right = cover_left + ICON_BOX_WIDTH;
    let text_left = cover_right + COVER_GAP_X;
    let text_right = inner_right - POPUP_INNER_MARGIN as i32;
    let text_width = (text_right - text_left).max(1) as f32;

    let bg_rgb = blended_bg_rgb(state.payload.dominant_rgb);

    let title_t = (alpha_t * (TEXT_MAIN_ALPHA_MAX / 255.0)).clamp(0.0, 1.0);
    let sub_t = (alpha_t * (TEXT_SUB_ALPHA_MAX / 255.0)).clamp(0.0, 1.0);
    let footer_t = (alpha_t * (FOOTER_ALPHA_MAX / 255.0)).clamp(0.0, 1.0);
    let note_t = (alpha_t * (FALLBACK_NOTE_ALPHA_MAX / 255.0)).clamp(0.0, 1.0);

    let title_rgb = blend_rgb(
        bg_rgb,
        [TEXT_MAIN_RGB.0, TEXT_MAIN_RGB.1, TEXT_MAIN_RGB.2],
        title_t,
    );
    let sub_rgb = blend_rgb(
        bg_rgb,
        [TEXT_SUB_RGB.0, TEXT_SUB_RGB.1, TEXT_SUB_RGB.2],
        sub_t,
    );
    let footer_rgb = blend_rgb(bg_rgb, [FOOTER_RGB.0, FOOTER_RGB.1, FOOTER_RGB.2], footer_t);
    let note_rgb = blend_rgb(
        bg_rgb,
        [
            FALLBACK_NOTE_RGB.0,
            FALLBACK_NOTE_RGB.1,
            FALLBACK_NOTE_RGB.2,
        ],
        note_t,
    );

    if state.cover_rgba.is_none() {
        let note_x = cover_left + ICON_BOX_WIDTH / 2;
        let note_y = margin + ICON_BOX_HEIGHT / 2;
        draw_text_centered(
            frame,
            frame_w,
            &state.icon_font,
            FALLBACK_NOTE_TEXT,
            FALLBACK_NOTE_FONT_SIZE,
            note_x as f32,
            note_y as f32,
            note_rgb,
            (note_t * 255.0).round() as u8,
        );
    }

    let mut y = (margin + TEXT_TOP_OFFSET as i32) as f32;

    draw_text_line(
        frame,
        frame_w,
        &state.title_font,
        &state.payload.title,
        ARTIST_FONT_SIZE,
        text_left as f32,
        y,
        title_rgb,
        (title_t * 255.0).round() as u8,
    );

    y += 19.0 + ARTIST_TRACK_SPACING;

    let track_one_line = truncate_text_to_width(
        &state.body_font,
        &state.payload.line1,
        TRACK_FONT_SIZE,
        text_width,
    );

    draw_text_line(
        frame,
        frame_w,
        &state.body_font,
        &track_one_line,
        TRACK_FONT_SIZE,
        text_left as f32,
        y,
        sub_rgb,
        (sub_t * 255.0).round() as u8,
    );

    if !state.payload.line2.is_empty() {
        y += 20.0 + ARTIST_TRACK_SPACING;

        let footer_top = popup_top + POPUP_HEIGHT as i32 - POPUP_INNER_MARGIN as i32 - 14;
        let max_h = (footer_top - TRACK_FOOTER_SPACING as i32) as f32 - y;

        if max_h > 0.0 {
            draw_text_wrapped_clipped(
                frame,
                frame_w,
                &state.body_font,
                &state.payload.line2,
                TRACK_FONT_SIZE,
                text_left as f32,
                y,
                text_width,
                max_h,
                sub_rgb,
                (sub_t * 255.0).round() as u8,
            );
        }
    }

    let footer_top = (popup_top + POPUP_HEIGHT as i32 - POPUP_INNER_MARGIN as i32 - 14) as f32;
    draw_text_line(
        frame,
        frame_w,
        &state.footer_font,
        &state.payload.footer,
        FOOTER_FONT_SIZE,
        text_left as f32,
        footer_top,
        footer_rgb,
        (footer_t * 255.0).round() as u8,
    );
}

pub fn draw_shadow(frame: &mut [u8], w: i32, h: i32, popup: RectI, alpha_t: f32) {
    let blur_steps = (SHADOW_BLUR as i32 / 3).max(4);
    let shadow_base_alpha = ((SHADOW_ALPHA_MAX * 0.82) * alpha_t).round() as u8;

    for i in 0..blur_steps {
        let expand = blur_steps - i;
        let rect = RectI {
            x: popup.x + SHADOW_OFFSET_X as i32 - expand / 2,
            y: popup.y + SHADOW_OFFSET_Y as i32 - expand / 2,
            w: popup.w + expand,
            h: popup.h + expand,
        };

        let a = ((shadow_base_alpha as f32) * (1.0 - i as f32 / blur_steps as f32) * 0.35).round()
            as u8;
        draw_rounded_rect(
            frame,
            w,
            h,
            rect,
            (POPUP_CORNER_RADIUS as i32 + expand / 2).max(1),
            [0, 0, 0],
            a,
        );
    }
}

pub fn draw_rounded_border(
    frame: &mut [u8],
    w: i32,
    h: i32,
    rect: RectI,
    radius: i32,
    thickness: i32,
    border_rgb: [u8; 3],
    border_alpha: u8,
    fill_rgb: [u8; 3],
    fill_alpha: u8,
) {
    draw_rounded_rect(frame, w, h, rect, radius, border_rgb, border_alpha);

    let inner = RectI {
        x: rect.x + thickness,
        y: rect.y + thickness,
        w: rect.w - thickness * 2,
        h: rect.h - thickness * 2,
    };

    if inner.w > 0 && inner.h > 0 {
        draw_rounded_rect(
            frame,
            w,
            h,
            inner,
            (radius - thickness).max(1),
            fill_rgb,
            fill_alpha,
        );
    }
}

pub fn draw_rounded_rect(
    frame: &mut [u8],
    w: i32,
    h: i32,
    rect: RectI,
    radius: i32,
    rgb: [u8; 3],
    alpha: u8,
) {
    let r = radius.max(1) as f32 - 0.5;

    for y in rect.y.max(0)..(rect.y + rect.h).min(h) {
        for x in rect.x.max(0)..(rect.x + rect.w).min(w) {
            let coverage = rounded_rect_coverage(x as f32 + 0.5, y as f32 + 0.5, rect, r);
            if coverage <= 0.0 {
                continue;
            }

            let a = ((alpha as f32) * coverage).round() as u8;
            blend_pixel(frame, w, x, y, rgb, a);
        }
    }
}

pub fn rounded_rect_coverage(px: f32, py: f32, rect: RectI, radius: f32) -> f32 {
    let left = rect.x as f32;
    let top = rect.y as f32;
    let right = (rect.x + rect.w) as f32;
    let bottom = (rect.y + rect.h) as f32;

    let r = radius.max(0.5);
    let inner_left = left + r;
    let inner_right = right - r;
    let inner_top = top + r;
    let inner_bottom = bottom - r;

    let cx = px.clamp(inner_left, inner_right);
    let cy = py.clamp(inner_top, inner_bottom);

    let dx = px - cx;
    let dy = py - cy;
    let dist = (dx * dx + dy * dy).sqrt();

    let aa = 1.0;
    ((r + aa - dist) / aa).clamp(0.0, 1.0)
}

pub fn draw_cover_rgba(
    frame: &mut [u8],
    w: i32,
    h: i32,
    cover_rgba: &[u8],
    cover_w: i32,
    cover_h: i32,
    dst_x: i32,
    dst_y: i32,
    dst_w: i32,
    dst_h: i32,
    radius: i32,
    opacity_t: f32,
) {
    if cover_w <= 0 || cover_h <= 0 || cover_rgba.is_empty() {
        return;
    }

    let opacity = opacity_t.clamp(0.0, 1.0);
    let r = radius.max(1) as f32 - 0.5;
    let rect = RectI {
        x: dst_x,
        y: dst_y,
        w: dst_w,
        h: dst_h,
    };

    for y in 0..dst_h {
        for x in 0..dst_w {
            let px = dst_x + x;
            let py = dst_y + y;

            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }

            let coverage = rounded_rect_coverage(px as f32 + 0.5, py as f32 + 0.5, rect, r);
            if coverage <= 0.0 {
                continue;
            }

            let sx = (x * cover_w / dst_w).clamp(0, cover_w - 1);
            let sy = (y * cover_h / dst_h).clamp(0, cover_h - 1);
            let i = ((sy * cover_w + sx) * 4) as usize;

            let sr = cover_rgba[i];
            let sg = cover_rgba[i + 1];
            let sb = cover_rgba[i + 2];
            let sa = ((cover_rgba[i + 3] as f32) * opacity * coverage).round() as u8;

            blend_pixel(frame, w, px, py, [sr, sg, sb], sa);
        }
    }
}

pub fn blend_pixel(frame: &mut [u8], stride_w: i32, x: i32, y: i32, rgb: [u8; 3], alpha: u8) {
    if x < 0 || y < 0 {
        return;
    }

    let idx = ((y * stride_w + x) * 4) as usize;
    if idx + 3 >= frame.len() {
        return;
    }

    let dst_b = frame[idx] as f32;
    let dst_g = frame[idx + 1] as f32;
    let dst_r = frame[idx + 2] as f32;
    let dst_a = frame[idx + 3] as f32 / 255.0;

    let src_a = alpha as f32 / 255.0;
    let src_r = rgb[0] as f32 * src_a;
    let src_g = rgb[1] as f32 * src_a;
    let src_b = rgb[2] as f32 * src_a;

    let out_a = src_a + dst_a * (1.0 - src_a);
    let out_r = src_r + dst_r * (1.0 - src_a);
    let out_g = src_g + dst_g * (1.0 - src_a);
    let out_b = src_b + dst_b * (1.0 - src_a);

    frame[idx] = out_b.round().clamp(0.0, 255.0) as u8;
    frame[idx + 1] = out_g.round().clamp(0.0, 255.0) as u8;
    frame[idx + 2] = out_r.round().clamp(0.0, 255.0) as u8;
    frame[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

pub fn blended_bg_rgb(dominant_rgb: Option<[u8; 3]>) -> [u8; 3] {
    let base = [BG_BASE_R, BG_BASE_G, BG_BASE_B];

    let Some(dc) = dominant_rgb else {
        return base;
    };

    let t = DOMINANT_BLEND_T.clamp(0.0, 1.0);

    [
        ((base[0] as f32 * (1.0 - t)) + (dc[0] as f32 * t)).round() as u8,
        ((base[1] as f32 * (1.0 - t)) + (dc[1] as f32 * t)).round() as u8,
        ((base[2] as f32 * (1.0 - t)) + (dc[2] as f32 * t)).round() as u8,
    ]
}

pub fn blend_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);

    [
        ((a[0] as f32 * (1.0 - t)) + (b[0] as f32 * t)).round() as u8,
        ((a[1] as f32 * (1.0 - t)) + (b[1] as f32 * t)).round() as u8,
        ((a[2] as f32 * (1.0 - t)) + (b[2] as f32 * t)).round() as u8,
    ]
}
