use crate::config::{COVER_SIZE_PX, MAX_ARTIST_CHARS, MAX_TRACK_CHARS};
use crate::models::PopupPayload;

use fontdue::Font;

use std::fs;
use std::time::Instant;

use super::text::truncate_text;

const ICON_BOX_WIDTH: i32 = COVER_SIZE_PX as i32;
const ICON_BOX_HEIGHT: i32 = COVER_SIZE_PX as i32;

pub struct PopupWindowState {
    pub payload: PopupPayload,
    pub title_font: Font,
    pub body_font: Font,
    pub footer_font: Font,
    pub icon_font: Font,
    pub cover_rgba: Option<Vec<u8>>,
    pub cover_width: i32,
    pub cover_height: i32,
    pub shown_at: Instant,
}

impl PopupWindowState {
    pub fn new(mut payload: PopupPayload) -> Self {
        payload.title = truncate_text(&payload.title, MAX_ARTIST_CHARS);
        payload.line1 = truncate_text(&payload.line1, MAX_TRACK_CHARS);

        let (cover_rgba, cover_width, cover_height) =
            load_cover_rgba(payload.cover_path.as_deref());

        Self {
            payload,
            title_font: load_system_font(&[
                r"C:\Windows\Fonts\seguisb.ttf",
                r"C:\Windows\Fonts\segoeui.ttf",
                r"C:\Windows\Fonts\arial.ttf",
                r"C:\Windows\Fonts\tahoma.ttf",
            ]),
            body_font: load_system_font(&[
                r"C:\Windows\Fonts\segoeui.ttf",
                r"C:\Windows\Fonts\arial.ttf",
                r"C:\Windows\Fonts\tahoma.ttf",
            ]),
            footer_font: load_system_font(&[
                r"C:\Windows\Fonts\segoeui.ttf",
                r"C:\Windows\Fonts\arial.ttf",
                r"C:\Windows\Fonts\tahoma.ttf",
            ]),
            icon_font: load_system_font(&[
                r"C:\Windows\Fonts\seguisym.ttf",
                r"C:\Windows\Fonts\segoeui.ttf",
                r"C:\Windows\Fonts\arial.ttf",
                r"C:\Windows\Fonts\tahoma.ttf",
            ]),
            cover_rgba,
            cover_width,
            cover_height,
            shown_at: Instant::now(),
        }
    }
}

fn load_cover_rgba(path: Option<&str>) -> (Option<Vec<u8>>, i32, i32) {
    let Some(path) = path else {
        return (None, 0, 0);
    };

    let Ok(bytes) = fs::read(path) else {
        return (None, 0, 0);
    };

    let Ok(img) = image::load_from_memory(&bytes) else {
        return (None, 0, 0);
    };

    let rgba = img
        .resize_to_fill(
            ICON_BOX_WIDTH as u32,
            ICON_BOX_HEIGHT as u32,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();

    let w = rgba.width() as i32;
    let h = rgba.height() as i32;

    (Some(rgba.into_raw()), w, h)
}

fn load_system_font(candidates: &[&str]) -> Font {
    for path in candidates {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                return font;
            }
        }
    }

    panic!("No usable system font found");
}
