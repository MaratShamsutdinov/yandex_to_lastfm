use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
use fontdue::Font;

use super::raster::blend_pixel;

pub fn draw_text_line(
    frame: &mut [u8],
    frame_w: i32,
    font: &Font,
    text: &str,
    px: f32,
    x: f32,
    y: f32,
    rgb: [u8; 3],
    alpha: u8,
) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x,
        y,
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });
    layout.append(&[font], &TextStyle::new(text, px, 0));

    for glyph in layout.glyphs() {
        let (metrics, bitmap) = font.rasterize_indexed(glyph.key.glyph_index, glyph.key.px);

        draw_glyph_bitmap(
            frame,
            frame_w,
            &bitmap,
            metrics.width as i32,
            metrics.height as i32,
            glyph.x.round() as i32,
            glyph.y.round() as i32,
            rgb,
            alpha,
        );
    }
}

pub fn draw_text_centered(
    frame: &mut [u8],
    frame_w: i32,
    font: &Font,
    text: &str,
    px: f32,
    center_x: f32,
    center_y: f32,
    rgb: [u8; 3],
    alpha: u8,
) {
    let width = text_width(font, text, px);
    let x = center_x - width / 2.0;
    let y = center_y - px * 0.55;
    draw_text_line(frame, frame_w, font, text, px, x, y, rgb, alpha);
}

pub fn draw_text_wrapped_clipped(
    frame: &mut [u8],
    frame_w: i32,
    font: &Font,
    text: &str,
    px: f32,
    x: f32,
    y: f32,
    max_width: f32,
    max_height: f32,
    rgb: [u8; 3],
    alpha: u8,
) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x,
        y,
        max_width: Some(max_width),
        max_height: Some(max_height),
        wrap_style: fontdue::layout::WrapStyle::Word,
        ..LayoutSettings::default()
    });
    layout.append(&[font], &TextStyle::new(text, px, 0));

    for glyph in layout.glyphs() {
        let (metrics, bitmap) = font.rasterize_indexed(glyph.key.glyph_index, glyph.key.px);

        draw_glyph_bitmap(
            frame,
            frame_w,
            &bitmap,
            metrics.width as i32,
            metrics.height as i32,
            glyph.x.round() as i32,
            glyph.y.round() as i32,
            rgb,
            alpha,
        );
    }
}

pub fn draw_glyph_bitmap(
    frame: &mut [u8],
    frame_w: i32,
    bitmap: &[u8],
    bw: i32,
    bh: i32,
    dst_x: i32,
    dst_y: i32,
    rgb: [u8; 3],
    alpha: u8,
) {
    if bw <= 0 || bh <= 0 {
        return;
    }

    let alpha_t = alpha as f32 / 255.0;

    for y in 0..bh {
        for x in 0..bw {
            let sx = x as usize;
            let sy = y as usize;
            let cov = bitmap[sy * bw as usize + sx];
            if cov == 0 {
                continue;
            }

            let px = dst_x + x;
            let py = dst_y + y;
            if px < 0 || py < 0 {
                continue;
            }

            let a = ((cov as f32 / 255.0) * alpha_t * 255.0).round() as u8;
            blend_pixel(frame, frame_w, px, py, rgb, a);
        }
    }
}

pub fn text_width(font: &Font, text: &str, px: f32) -> f32 {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        x: 0.0,
        y: 0.0,
        max_width: None,
        max_height: None,
        ..LayoutSettings::default()
    });
    layout.append(&[font], &TextStyle::new(text, px, 0));

    let mut max_right = 0.0_f32;

    for glyph in layout.glyphs() {
        let (metrics, _) = font.rasterize_indexed(glyph.key.glyph_index, glyph.key.px);
        let right = glyph.x + metrics.width as f32;
        if right > max_right {
            max_right = right;
        }
    }

    max_right
}

pub fn truncate_text(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }

    let trimmed: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{trimmed}…")
}

pub fn truncate_text_to_width(font: &Font, text: &str, px: f32, max_width: f32) -> String {
    if text_width(font, text, px) <= max_width {
        return text.to_string();
    }

    let ellipsis = "…";
    let ellipsis_w = text_width(font, ellipsis, px);
    let mut out = String::new();

    for ch in text.chars() {
        let mut candidate = out.clone();
        candidate.push(ch);

        if text_width(font, &candidate, px) + ellipsis_w > max_width {
            break;
        }

        out.push(ch);
    }

    out.push('…');
    out
}
