use crate::app_config::{
    clear_app_config, default_app_config, load_app_config, save_app_config, AppConfig, LastfmConfig,
};
use crate::server::validate_lastfm_credentials_quick;

use std::ffi::c_void;
use std::mem::zeroed;
use std::process::Command;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DEFAULT_PITCH, FF_DONTCARE, FW_NORMAL, HFONT, OUT_DEFAULT_PRECIS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow, LoadCursorW, MessageBoxW,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, SetWindowTextW, ShowWindow, TranslateMessage,
    CREATESTRUCTW, CW_USEDEFAULT, ES_AUTOHSCROLL, ES_LEFT, ES_PASSWORD, GWLP_USERDATA, HMENU,
    IDC_ARROW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND,
    WM_CREATE, WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CAPTION,
    WS_CHILD, WS_CLIPCHILDREN, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};

#[link(name = "user32")]
unsafe extern "system" {
    fn EnableWindow(hwnd: HWND, benable: i32) -> i32;
    fn SetFocus(hwnd: HWND) -> HWND;
    fn UpdateWindow(hwnd: HWND) -> i32;
}

const SETTINGS_WINDOW_CLASS_NAME: &str = "YaMusicLastFmSettingsWindowClass";
const SETTINGS_WINDOW_TITLE: &str = "YaMusic → Last.fm Settings";

const ID_EDIT_API_KEY: isize = 1001;
const ID_EDIT_API_SECRET: isize = 1002;
const ID_EDIT_USERNAME: isize = 1003;
const ID_EDIT_PASSWORD: isize = 1004;

const ID_BUTTON_SAVE: isize = 2001;
const ID_BUTTON_CANCEL: isize = 2002;
const ID_BUTTON_TEST: isize = 2003;
const ID_BUTTON_OPEN_API: isize = 2004;
const ID_BUTTON_CLEAR: isize = 2005;

const WINDOW_W: i32 = 520;
const WINDOW_H: i32 = 540;

const CONTENT_LEFT: i32 = 22;
const LABEL_X: i32 = 22;
const EDIT_X: i32 = 160;
const EDIT_W: i32 = 320;
const ROW_H: i32 = 26;

const HEADER_Y: i32 = 18;
const SUBHEADER_Y: i32 = 46;
const HELP_Y: i32 = 66;

const TOP_Y: i32 = 280;
const ROW_STEP_Y: i32 = 46;

const BUTTON_W: i32 = 100;
const BUTTON_H: i32 = 30;
const BUTTON_GAP: i32 = 8;
const BUTTON_Y: i32 = 455;

struct SettingsWindowState {
    initial_config: AppConfig,
    result_config: Option<AppConfig>,

    title_label: HWND,
    subtitle_label: HWND,
    help_label_1: HWND,
    help_label_2: HWND,
    help_label_3: HWND,
    help_label_4: HWND,
    help_label_5: HWND,
    help_label_6: HWND,

    label_api_key: HWND,
    label_api_secret: HWND,
    label_username: HWND,
    label_password: HWND,

    edit_api_key: HWND,
    edit_api_secret: HWND,
    edit_username: HWND,
    edit_password: HWND,

    button_save: HWND,
    button_cancel: HWND,
    button_test: HWND,
    button_open_api: HWND,
    button_clear: HWND,

    ui_font: HFONT,
    title_font: HFONT,
}

impl SettingsWindowState {
    fn new(initial_config: AppConfig) -> Self {
        Self {
            initial_config,
            result_config: None,

            title_label: null_mut(),
            subtitle_label: null_mut(),
            help_label_1: null_mut(),
            help_label_2: null_mut(),
            help_label_3: null_mut(),
            help_label_4: null_mut(),
            help_label_5: null_mut(),
            help_label_6: null_mut(),

            label_api_key: null_mut(),
            label_api_secret: null_mut(),
            label_username: null_mut(),
            label_password: null_mut(),

            edit_api_key: null_mut(),
            edit_api_secret: null_mut(),
            edit_username: null_mut(),
            edit_password: null_mut(),

            button_save: null_mut(),
            button_cancel: null_mut(),
            button_test: null_mut(),
            button_open_api: null_mut(),
            button_clear: null_mut(),

            ui_font: null_mut(),
            title_font: null_mut(),
        }
    }
}

pub fn prompt_lastfm_settings_if_missing() -> Result<AppConfig, String> {
    if let Some(config) = load_app_config()? {
        if config.is_complete() {
            return Ok(config);
        }
    }

    let initial_config = load_app_config()?.unwrap_or_else(default_app_config);

    let maybe_config = show_lastfm_settings_window(initial_config)?;
    let config = maybe_config.ok_or_else(|| "Last.fm settings were not provided".to_string())?;

    save_app_config(&config)?;
    Ok(config)
}

pub fn show_lastfm_settings_window(initial_config: AppConfig) -> Result<Option<AppConfig>, String> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(null());
        if hinstance.is_null() {
            return Err("GetModuleHandleW failed".to_string());
        }

        register_settings_window_class(hinstance);

        let state_ptr = Box::into_raw(Box::new(SettingsWindowState::new(initial_config)));

        let class_name = to_wide_null(SETTINGS_WINDOW_CLASS_NAME);
        let window_title = to_wide_null(SETTINGS_WINDOW_TITLE);

        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_W,
            WINDOW_H,
            null_mut(),
            null_mut::<c_void>() as HMENU,
            hinstance,
            state_ptr as *const c_void,
        );

        if hwnd.is_null() {
            let _ = Box::from_raw(state_ptr);
            return Err("CreateWindowExW failed".to_string());
        }

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = zeroed();

        while IsWindow(hwnd) != 0 {
            let ret = GetMessageW(&mut msg, null_mut(), 0, 0);
            if ret <= 0 {
                break;
            }

            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let state = Box::from_raw(state_ptr);
        Ok(state.result_config)
    }
}

unsafe extern "system" fn settings_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let createstruct = lparam as *const CREATESTRUCTW;
            if createstruct.is_null() {
                return 0;
            }

            let lp_create_params = (*createstruct).lpCreateParams as *mut SettingsWindowState;
            if lp_create_params.is_null() {
                return 0;
            }

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp_create_params as isize);
            1
        }
        WM_CREATE => {
            if let Err(err) = create_settings_controls(hwnd) {
                show_error_box("Create settings controls", &err);
                DestroyWindow(hwnd);
                return 0;
            }

            fill_controls_from_initial_config(hwnd);
            focus_first_field(hwnd);
            0
        }
        WM_COMMAND => {
            let command_id = loword(wparam) as isize;

            if command_id == ID_BUTTON_TEST {
                handle_test_button(hwnd);
                return 0;
            }

            if command_id == ID_BUTTON_SAVE {
                handle_save_button(hwnd);
                return 0;
            }

            if command_id == ID_BUTTON_CANCEL {
                DestroyWindow(hwnd);
                return 0;
            }

            if command_id == ID_BUTTON_OPEN_API {
                open_lastfm_api_page();
                return 0;
            }

            if command_id == ID_BUTTON_CLEAR {
                handle_clear_button(hwnd);
                return 0;
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => 0,
        WM_NCDESTROY => {
            let state_ptr = get_state_ptr(hwnd);
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;

                if !state.ui_font.is_null() {
                    DeleteObject(state.ui_font as *mut c_void);
                    state.ui_font = null_mut();
                }

                if !state.title_font.is_null() {
                    DeleteObject(state.title_font as *mut c_void);
                    state.title_font = null_mut();
                }
            }

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn register_settings_window_class(hinstance: HINSTANCE) {
    let class_name = to_wide_null(SETTINGS_WINDOW_CLASS_NAME);

    let wnd_class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(settings_wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: null_mut(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: 16 as *mut c_void,
        lpszMenuName: null(),
        lpszClassName: class_name.as_ptr(),
    };

    RegisterClassW(&wnd_class);
}

unsafe fn create_settings_controls(hwnd: HWND) -> Result<(), String> {
    let state_ptr = get_state_ptr(hwnd);
    if state_ptr.is_null() {
        return Err("settings state is null".to_string());
    }

    let state = &mut *state_ptr;

    state.ui_font = create_ui_font(18, FW_NORMAL as i32)?;
    state.title_font = create_ui_font(24, 600)?;

    state.title_label = create_label(
        hwnd,
        "Connect your Last.fm account",
        CONTENT_LEFT,
        HEADER_Y,
        320,
        24,
    )?;
    state.subtitle_label = create_label(
        hwnd,
        "These credentials are stored locally on this PC.",
        CONTENT_LEFT,
        SUBHEADER_Y,
        420,
        18,
    )?;

    state.help_label_1 = create_label(
        hwnd,
        "1) Open https://www.last.fm/api/account/create",
        CONTENT_LEFT,
        HELP_Y,
        460,
        18,
    )?;

    state.button_open_api = create_button(
        hwnd,
        "Open Last.fm API page",
        ID_BUTTON_OPEN_API,
        CONTENT_LEFT + 18,
        HELP_Y + 24,
        220,
        28,
    )?;

    state.help_label_2 = create_label(
        hwnd,
        "2) Create an API account and copy API Key and API Secret",
        CONTENT_LEFT,
        HELP_Y + 60,
        460,
        18,
    )?;

    state.help_label_3 = create_label(
        hwnd,
        "Suggested values:",
        CONTENT_LEFT,
        HELP_Y + 86,
        460,
        18,
    )?;

    let sv_y1 = HELP_Y + 112;
    let sv_y2 = HELP_Y + 142;
    let sv_y3 = HELP_Y + 172;

    create_label(hwnd, "App name:", CONTENT_LEFT, sv_y1, 120, 18)?;
    create_label(hwnd, "Callback URL:", CONTENT_LEFT, sv_y2, 120, 18)?;
    create_label(hwnd, "Homepage:", CONTENT_LEFT, sv_y3, 120, 18)?;

    state.help_label_4 = create_edit(hwnd, 0, CONTENT_LEFT + 120, sv_y1 - 2, 300, 22, false, true)?;
    set_edit_text(state.help_label_4, "YaMusic Last.fm Popup");

    state.help_label_5 = create_edit(hwnd, 0, CONTENT_LEFT + 120, sv_y2 - 2, 300, 22, false, true)?;
    set_edit_text(state.help_label_5, "Empty");

    state.help_label_6 = create_edit(hwnd, 0, CONTENT_LEFT + 120, sv_y3 - 2, 300, 22, false, true)?;
    set_edit_text(state.help_label_6, "http://127.0.0.1");

    state.label_api_key = create_label(hwnd, "API Key", LABEL_X, TOP_Y + 3, 120, 20)?;
    state.label_api_secret =
        create_label(hwnd, "API Secret", LABEL_X, TOP_Y + ROW_STEP_Y + 3, 120, 20)?;
    state.label_username = create_label(
        hwnd,
        "Username",
        LABEL_X,
        TOP_Y + ROW_STEP_Y * 2 + 3,
        120,
        20,
    )?;
    state.label_password = create_label(
        hwnd,
        "Password",
        LABEL_X,
        TOP_Y + ROW_STEP_Y * 3 + 3,
        120,
        20,
    )?;

    state.edit_api_key = create_edit(
        hwnd,
        ID_EDIT_API_KEY,
        EDIT_X,
        TOP_Y,
        EDIT_W,
        ROW_H,
        false,
        false,
    )?;
    state.edit_api_secret = create_edit(
        hwnd,
        ID_EDIT_API_SECRET,
        EDIT_X,
        TOP_Y + ROW_STEP_Y,
        EDIT_W,
        ROW_H,
        false,
        false,
    )?;
    state.edit_username = create_edit(
        hwnd,
        ID_EDIT_USERNAME,
        EDIT_X,
        TOP_Y + ROW_STEP_Y * 2,
        EDIT_W,
        ROW_H,
        false,
        false,
    )?;
    state.edit_password = create_edit(
        hwnd,
        ID_EDIT_PASSWORD,
        EDIT_X,
        TOP_Y + ROW_STEP_Y * 3,
        EDIT_W,
        ROW_H,
        true,
        false,
    )?;

    let buttons_total_w = BUTTON_W * 4 + BUTTON_GAP * 3;
    let buttons_x = WINDOW_W - buttons_total_w - 36;

    state.button_save = create_button(
        hwnd,
        "Save",
        ID_BUTTON_SAVE,
        buttons_x,
        BUTTON_Y,
        BUTTON_W,
        BUTTON_H,
    )?;
    state.button_test = create_button(
        hwnd,
        "Test",
        ID_BUTTON_TEST,
        buttons_x + (BUTTON_W + BUTTON_GAP),
        BUTTON_Y,
        BUTTON_W,
        BUTTON_H,
    )?;
    state.button_clear = create_button(
        hwnd,
        "Clear",
        ID_BUTTON_CLEAR,
        buttons_x + (BUTTON_W + BUTTON_GAP) * 2,
        BUTTON_Y,
        BUTTON_W,
        BUTTON_H,
    )?;
    state.button_cancel = create_button(
        hwnd,
        "Cancel",
        ID_BUTTON_CANCEL,
        buttons_x + (BUTTON_W + BUTTON_GAP) * 3,
        BUTTON_Y,
        BUTTON_W,
        BUTTON_H,
    )?;

    apply_font(state.title_label, state.title_font);
    apply_font(state.subtitle_label, state.ui_font);

    apply_font(state.help_label_1, state.ui_font);
    apply_font(state.help_label_2, state.ui_font);
    apply_font(state.help_label_3, state.ui_font);
    apply_font(state.help_label_4, state.ui_font);
    apply_font(state.help_label_5, state.ui_font);
    apply_font(state.help_label_6, state.ui_font);

    apply_font(state.button_open_api, state.ui_font);

    apply_font(state.label_api_key, state.ui_font);
    apply_font(state.label_api_secret, state.ui_font);
    apply_font(state.label_username, state.ui_font);
    apply_font(state.label_password, state.ui_font);

    apply_font(state.edit_api_key, state.ui_font);
    apply_font(state.edit_api_secret, state.ui_font);
    apply_font(state.edit_username, state.ui_font);
    apply_font(state.edit_password, state.ui_font);

    apply_font(state.button_save, state.ui_font);
    apply_font(state.button_cancel, state.ui_font);
    apply_font(state.button_test, state.ui_font);
    apply_font(state.button_clear, state.ui_font);

    Ok(())
}

unsafe fn create_ui_font(height: i32, weight: i32) -> Result<HFONT, String> {
    let face_name = to_wide_null("Segoe UI");

    let font = CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        (DEFAULT_PITCH | FF_DONTCARE) as u32,
        face_name.as_ptr(),
    );

    if font.is_null() {
        return Err("CreateFontW failed".to_string());
    }

    Ok(font)
}

unsafe fn apply_font(hwnd: HWND, font: HFONT) {
    if hwnd.is_null() || font.is_null() {
        return;
    }

    SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
}

unsafe fn fill_controls_from_initial_config(hwnd: HWND) {
    let state_ptr = get_state_ptr(hwnd);
    if state_ptr.is_null() {
        return;
    }

    let state = &*state_ptr;
    let normalized = state.initial_config.normalized();

    set_edit_text(state.edit_api_key, &normalized.lastfm.api_key);
    set_edit_text(state.edit_api_secret, &normalized.lastfm.api_secret);
    set_edit_text(state.edit_username, &normalized.lastfm.username);
    set_edit_text(state.edit_password, &normalized.lastfm.password);
}

unsafe fn focus_first_field(hwnd: HWND) {
    let state_ptr = get_state_ptr(hwnd);
    if state_ptr.is_null() {
        return;
    }

    let state = &*state_ptr;
    if !state.edit_api_key.is_null() {
        SetFocus(state.edit_api_key);
    }
}

unsafe fn handle_test_button(hwnd: HWND) {
    set_buttons_enabled(hwnd, false);

    let result = match read_config_from_controls(hwnd) {
        Ok(config) => {
            let rt = tokio::runtime::Runtime::new();
            match rt {
                Ok(rt) => {
                    rt.block_on(async { validate_lastfm_credentials_quick(&config.lastfm).await })
                }
                Err(e) => Err(format!("tokio runtime error: {e}")),
            }
        }
        Err(e) => Err(e),
    };

    set_buttons_enabled(hwnd, true);

    match result {
        Ok(_) => show_info_box("Last.fm validation", "Connection OK."),
        Err(e) => show_error_box("Last.fm validation", &e),
    }
}

unsafe fn handle_save_button(hwnd: HWND) {
    set_buttons_enabled(hwnd, false);

    let final_result = match read_config_from_controls(hwnd) {
        Ok(config) => {
            let rt = tokio::runtime::Runtime::new();
            match rt {
                Ok(rt) => {
                    match rt
                        .block_on(async { validate_lastfm_credentials_quick(&config.lastfm).await })
                    {
                        Ok(_) => Ok(config),
                        Err(e) => Err(e),
                    }
                }
                Err(e) => Err(format!("tokio runtime error: {e}")),
            }
        }
        Err(e) => Err(e),
    };

    set_buttons_enabled(hwnd, true);

    match final_result {
        Ok(config) => {
            let state_ptr = get_state_ptr(hwnd);
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                state.result_config = Some(config);
            }

            DestroyWindow(hwnd);
        }
        Err(e) => {
            show_error_box("Save Last.fm settings", &e);
        }
    }
}

unsafe fn handle_clear_button(hwnd: HWND) {
    set_buttons_enabled(hwnd, false);

    let result = clear_app_config();

    if result.is_ok() {
        let state_ptr = get_state_ptr(hwnd);
        if !state_ptr.is_null() {
            let state = &mut *state_ptr;

            set_edit_text(state.edit_api_key, "");
            set_edit_text(state.edit_api_secret, "");
            set_edit_text(state.edit_username, "");
            set_edit_text(state.edit_password, "");

            state.initial_config = default_app_config();
            state.result_config = None;
        }
    }

    set_buttons_enabled(hwnd, true);

    match result {
        Ok(_) => show_info_box(
            "Clear Last.fm settings",
            "Saved WinApp Last.fm data cleared.",
        ),
        Err(e) => show_error_box("Clear Last.fm settings", &e),
    }
}

unsafe fn read_config_from_controls(hwnd: HWND) -> Result<AppConfig, String> {
    let state_ptr = get_state_ptr(hwnd);
    if state_ptr.is_null() {
        return Err("settings state is null".to_string());
    }

    let state = &*state_ptr;

    let config = AppConfig {
        lastfm: LastfmConfig {
            api_key: get_edit_text(state.edit_api_key)?,
            api_secret: get_edit_text(state.edit_api_secret)?,
            username: get_edit_text(state.edit_username)?,
            password: get_edit_text(state.edit_password)?,
            session_key: String::new(),
            synced_from_extension: false,
        },
        launch_on_startup: state.initial_config.launch_on_startup,
    }
    .normalized();

    if !config.has_full_credentials() && !config.has_companion_auth() {
        return Err(
            "Please provide either full Last.fm credentials or a synced companion session."
                .to_string(),
        );
    }

    Ok(config)
}

unsafe fn set_buttons_enabled(hwnd: HWND, enabled: bool) {
    let state_ptr = get_state_ptr(hwnd);
    if state_ptr.is_null() {
        return;
    }

    let state = &*state_ptr;
    let enabled_flag = if enabled { 1 } else { 0 };

    if !state.button_save.is_null() {
        EnableWindow(state.button_save, enabled_flag);
    }
    if !state.button_cancel.is_null() {
        EnableWindow(state.button_cancel, enabled_flag);
    }
    if !state.button_test.is_null() {
        EnableWindow(state.button_test, enabled_flag);
    }
    if !state.button_clear.is_null() {
        EnableWindow(state.button_clear, enabled_flag);
    }
}

unsafe fn create_label(
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<HWND, String> {
    let class_name = to_wide_null("STATIC");
    let text_w = to_wide_null(text);

    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        text_w.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        w,
        h,
        parent,
        null_mut(),
        null_mut(),
        null_mut(),
    );

    if hwnd.is_null() {
        return Err(format!("CreateWindowExW STATIC failed for '{text}'"));
    }

    Ok(hwnd)
}

unsafe fn create_edit(
    parent: HWND,
    control_id: isize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    password: bool,
    readonly: bool,
) -> Result<HWND, String> {
    let class_name = to_wide_null("EDIT");
    let empty_text = to_wide_null("");

    let style = WS_CHILD
        | WS_VISIBLE
        | WS_TABSTOP
        | WS_BORDER
        | (ES_LEFT as u32)
        | (ES_AUTOHSCROLL as u32)
        | if password { ES_PASSWORD as u32 } else { 0 }
        | if readonly { 0x0800 } else { 0 }; // ES_READONLY

    let hwnd = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        class_name.as_ptr(),
        empty_text.as_ptr(),
        style,
        x,
        y,
        w,
        h,
        parent,
        control_id as HMENU,
        null_mut(),
        null_mut(),
    );

    if hwnd.is_null() {
        return Err(format!(
            "CreateWindowExW EDIT failed for control id {control_id}"
        ));
    }

    Ok(hwnd)
}

unsafe fn create_button(
    parent: HWND,
    text: &str,
    control_id: isize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<HWND, String> {
    let class_name = to_wide_null("BUTTON");
    let text_w = to_wide_null(text);

    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        text_w.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        x,
        y,
        w,
        h,
        parent,
        control_id as HMENU,
        null_mut(),
        null_mut(),
    );

    if hwnd.is_null() {
        return Err(format!("CreateWindowExW BUTTON failed for '{text}'"));
    }

    Ok(hwnd)
}

unsafe fn get_edit_text(hwnd: HWND) -> Result<String, String> {
    if hwnd.is_null() {
        return Err("edit control handle is null".to_string());
    }

    let len = GetWindowTextLengthW(hwnd);
    if len < 0 {
        return Err("GetWindowTextLengthW failed".to_string());
    }

    let mut buf = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if copied < 0 {
        return Err("GetWindowTextW failed".to_string());
    }

    Ok(String::from_utf16_lossy(&buf[..copied as usize]))
}

unsafe fn set_edit_text(hwnd: HWND, value: &str) {
    if hwnd.is_null() {
        return;
    }

    let value_w = to_wide_null(value);
    SetWindowTextW(hwnd, value_w.as_ptr());
}

unsafe fn get_state_ptr(hwnd: HWND) -> *mut SettingsWindowState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SettingsWindowState
}

fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn show_info_box(title: &str, text: &str) {
    let title_w = to_wide_null(title);
    let text_w = to_wide_null(text);

    unsafe {
        MessageBoxW(
            null_mut(),
            text_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

fn show_error_box(title: &str, text: &str) {
    let title_w = to_wide_null(title);
    let text_w = to_wide_null(text);

    unsafe {
        MessageBoxW(
            null_mut(),
            text_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn open_lastfm_api_page() {
    let url = "https://www.last.fm/api/account/create";

    if Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .is_err()
    {
        show_error_box(
            "Open Last.fm API page",
            "Could not open browser.\n\nPlease open manually:\nhttps://www.last.fm/api/account/create",
        );
    }
}
