pub const API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

// ====== LOCAL SERVER =========================================================
pub const SERVER_BIND_ADDR: &str = "127.0.0.1:5000";

// ====== RETRY / NETWORK ======================================================
pub const SESSION_KEY_RETRY_COUNT: u32 = 5;
pub const SESSION_KEY_RETRY_BASE_DELAY_SECS: u64 = 2;
pub const COVER_DOWNLOAD_TIMEOUT_MS: u64 = 1800;

// ====== PLAYBACK / SCROBBLE ==================================================
pub const SCROBBLE_AFTER_SECS: i64 = 30;
pub const POPUP_VISIBLE_SECS: u64 = 7;

// ====== POPUP WINDOW GEOMETRY ================================================
pub const POPUP_WIDTH: f32 = 344.0;
pub const POPUP_HEIGHT: f32 = 84.0;
pub const POPUP_MARGIN_RIGHT: i32 = 20;
pub const POPUP_MARGIN_BOTTOM: i32 = 80;

// ====== POPUP ANIMATION ======================================================
pub const POPUP_FADE_IN_MS: u64 = 180;
pub const POPUP_FADE_OUT_MS: u64 = 220;
pub const POPUP_SLIDE_PX: f32 = 36.0;

// ====== COVER ANIMATION ======================================================
pub const COVER_ANIM_MS: u64 = 660;
pub const COVER_RISE_PX: f32 = 14.0;

// ====== TEXT LIMITS ==========================================================
pub const MAX_ARTIST_CHARS: usize = 34;
pub const MAX_TRACK_CHARS: usize = 42;

// ====== UI SIZES / SPACING ===================================================
pub const COVER_SIZE_PX: f32 = 64.0;
pub const COVER_CORNER_RADIUS: u8 = 12;
pub const POPUP_CORNER_RADIUS: u8 = 18;
pub const POPUP_INNER_MARGIN: i8 = 10;
pub const CONTENT_GAP_X: f32 = 10.0;

pub const TEXT_TOP_OFFSET: f32 = 2.0;
pub const ARTIST_TRACK_SPACING: f32 = 4.0;
pub const TRACK_FOOTER_SPACING: f32 = 6.0;

// ====== UI FONTS =============================================================
pub const ARTIST_FONT_SIZE: f32 = 13.0;
pub const TRACK_FONT_SIZE: f32 = 12.5;
pub const FOOTER_FONT_SIZE: f32 = 9.0;
pub const FALLBACK_NOTE_FONT_SIZE: f32 = 30.0;

// ====== FRAME TIMING =========================================================
pub const FRAME_REPAINT_MS: u64 = 16;

// ====== COLORS / ALPHA =======================================================
pub const BG_BASE_R: u8 = 18;
pub const BG_BASE_G: u8 = 18;
pub const BG_BASE_B: u8 = 20;

pub const BG_ALPHA_MAX: f32 = 165.0;
pub const TEXT_MAIN_ALPHA_MAX: f32 = 255.0;
pub const TEXT_SUB_ALPHA_MAX: f32 = 220.0;
pub const FOOTER_ALPHA_MAX: f32 = 180.0;
pub const FALLBACK_NOTE_ALPHA_MAX: f32 = 180.0;
pub const SHADOW_ALPHA_MAX: f32 = 10.0;
pub const DOMINANT_BLEND_T: f32 = 0.22;

pub const TEXT_MAIN_RGB: (u8, u8, u8) = (255, 255, 255);
pub const TEXT_SUB_RGB: (u8, u8, u8) = (220, 220, 220);
pub const FOOTER_RGB: (u8, u8, u8) = (170, 170, 170);
pub const FALLBACK_NOTE_RGB: (u8, u8, u8) = (220, 220, 220);

// ====== SHADOW ===============================================================
pub const SHADOW_OFFSET_X: i8 = 0;
pub const SHADOW_OFFSET_Y: i8 = 1;
pub const SHADOW_BLUR: u8 = 6;

// ====== DOMINANT COLOR DETECTION =============================================
pub const DOMINANT_SAMPLE_GRID: u32 = 24;
pub const DOMINANT_MIN_BRIGHTNESS: u8 = 24;
pub const DOMINANT_MIN_SATURATION: u8 = 12;

// ====== TEXT / APP STRINGS ===================================================
pub const APP_WINDOW_TITLE: &str = "YaMusic Last.fm Popup";
pub const APP_FOOTER_TEXT: &str = "YaMusic Last.fm Popup";
pub const TEST_ARTIST_TEXT: &str = "Test Artist";
pub const TEST_TRACK_TEXT: &str = "Test Track With A Bit Longer Name";
pub const FALLBACK_NOTE_TEXT: &str = "♪";

// ====== TRAY =================================================================
pub const TRAY_TOOLTIP: &str = "YaMusic Last.fm Popup";
pub const TRAY_MENU_STATUS_NOT_DETECTED_TEXT: &str = "Chrome extension: not detected";
pub const TRAY_MENU_STATUS_CONNECTED_TEXT: &str = "Chrome extension: connected";
pub const TRAY_MENU_INSTALL_TEXT: &str = "Install Chrome Extension";
pub const TRAY_MENU_OPEN_EXTENSIONS_TEXT: &str = "Open extension page";
pub const TRAY_MENU_LASTFM_SETTINGS_TEXT: &str = "Last.fm settings";
pub const TRAY_MENU_VALIDATE_LASTFM_TEXT: &str = "Validate Last.fm connection";
pub const TRAY_MENU_SHOW_TEXT: &str = "Show test popup";
pub const TRAY_MENU_EXIT_TEXT: &str = "Exit";
pub const TRAY_MENU_AUTOSTART_ENABLE_TEXT: &str = "Enable autostart";
pub const TRAY_MENU_AUTOSTART_DISABLE_TEXT: &str = "Disable autostart";
