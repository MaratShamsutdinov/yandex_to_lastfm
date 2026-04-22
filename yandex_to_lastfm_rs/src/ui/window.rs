use crate::config::{
    APP_WINDOW_TITLE, FRAME_REPAINT_MS, POPUP_HEIGHT, POPUP_MARGIN_BOTTOM, POPUP_MARGIN_RIGHT,
    POPUP_SLIDE_PX, POPUP_WIDTH,
};
use crate::models::PopupPayload;

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, GetWindowLongPtrW, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW,
    SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, HMENU,
    IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE, WM_CREATE, WM_DESTROY,
    WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

use super::anim::current_anim_values;
use super::raster::{raster_popup_frame, raster_text_layers, SHADOW_PAD, WINDOW_H, WINDOW_W};
use super::state::PopupWindowState;

#[repr(C)]
struct WinPoint {
    x: i32,
    y: i32,
}

#[repr(C)]
struct WinSize {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct BlendFunction {
    blend_op: u8,
    blend_flags: u8,
    source_constant_alpha: u8,
    alpha_format: u8,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetDC(hwnd: HWND) -> HDC;
    fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
    fn UpdateLayeredWindow(
        hwnd: HWND,
        hdc_dst: HDC,
        ppt_dst: *const WinPoint,
        psize: *const WinSize,
        hdc_src: HDC,
        ppt_src: *const WinPoint,
        cr_key: u32,
        pblend: *const BlendFunction,
        dw_flags: u32,
    ) -> i32;
}

const POPUP_TIMER_ID: usize = 1;
const AC_SRC_OVER: u8 = 0;
const AC_SRC_ALPHA: u8 = 1;
const ULW_ALPHA: u32 = 0x00000002;

pub fn show_popup_window(payload: PopupPayload) -> Result<(), String> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(null());
        if hinstance.is_null() {
            return Err("GetModuleHandleW failed".to_string());
        }

        register_popup_class(hinstance);

        let state = Box::new(PopupWindowState::new(payload));
        let state_ptr = Box::into_raw(state);

        let class_name = to_wide_null("YaMusicLastFmPopupWindowClass");
        let window_title = to_wide_null(APP_WINDOW_TITLE);
        let (x, y) = popup_window_outer_pos();

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
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

        ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

unsafe extern "system" fn popup_wndproc(
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

            let lp_create_params = (*createstruct).lpCreateParams as *mut PopupWindowState;
            if lp_create_params.is_null() {
                return 0;
            }

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lp_create_params as isize);
            1
        }
        WM_CREATE => {
            render_and_present(hwnd);
            SetTimer(hwnd, POPUP_TIMER_ID, FRAME_REPAINT_MS as u32, None);
            0
        }
        WM_TIMER => {
            if wparam == POPUP_TIMER_ID {
                if !render_and_present(hwnd) {
                    KillTimer(hwnd, POPUP_TIMER_ID);
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => 0,
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            let state_ptr = get_state_ptr(hwnd);
            if !state_ptr.is_null() {
                let _ = Box::from_raw(state_ptr);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn render_and_present(hwnd: HWND) -> bool {
    let state_ptr = get_state_ptr(hwnd);
    if state_ptr.is_null() {
        return false;
    }

    let state = &*state_ptr;
    let (alpha_t, slide_t, cover_t, done) = current_anim_values(state);

    if done {
        return false;
    }

    let mut frame = vec![0u8; (WINDOW_W * WINDOW_H * 4) as usize];
    raster_popup_frame(&mut frame, WINDOW_W, WINDOW_H, state, alpha_t, cover_t);
    raster_text_layers(&mut frame, WINDOW_W, WINDOW_H, state, alpha_t);

    let hdc_screen = GetDC(null_mut());
    if hdc_screen.is_null() {
        return false;
    }

    let hdc_mem = CreateCompatibleDC(hdc_screen);
    if hdc_mem.is_null() {
        ReleaseDC(null_mut(), hdc_screen);
        return false;
    }

    let mut bits_ptr: *mut c_void = null_mut();
    let mut bmi: BITMAPINFO = zeroed();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: WINDOW_W,
        biHeight: -WINDOW_H,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..zeroed()
    };

    let hbitmap: HBITMAP =
        CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits_ptr, null_mut(), 0);

    if hbitmap.is_null() || bits_ptr.is_null() {
        if !hbitmap.is_null() {
            DeleteObject(hbitmap as HGDIOBJ);
        }
        DeleteDC(hdc_mem);
        ReleaseDC(null_mut(), hdc_screen);
        return false;
    }

    let old_obj = SelectObject(hdc_mem, hbitmap as HGDIOBJ);

    std::ptr::copy_nonoverlapping(frame.as_ptr(), bits_ptr as *mut u8, frame.len());

    let (base_x, base_y) = popup_window_outer_pos();
    let offset_x = ((1.0 - slide_t) * POPUP_SLIDE_PX).round() as i32;

    let dst_pt = WinPoint {
        x: base_x + offset_x,
        y: base_y,
    };
    let src_pt = WinPoint { x: 0, y: 0 };
    let size = WinSize {
        cx: WINDOW_W,
        cy: WINDOW_H,
    };

    let blend = BlendFunction {
        blend_op: AC_SRC_OVER,
        blend_flags: 0,
        source_constant_alpha: 255,
        alpha_format: AC_SRC_ALPHA,
    };

    let ok = UpdateLayeredWindow(
        hwnd, hdc_screen, &dst_pt, &size, hdc_mem, &src_pt, 0, &blend, ULW_ALPHA,
    );

    SelectObject(hdc_mem, old_obj);
    DeleteObject(hbitmap as HGDIOBJ);
    DeleteDC(hdc_mem);
    ReleaseDC(null_mut(), hdc_screen);

    ok != 0
}

unsafe fn register_popup_class(hinstance: HINSTANCE) {
    let class_name = to_wide_null("YaMusicLastFmPopupWindowClass");

    let wnd_class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(popup_wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: null_mut(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class_name.as_ptr(),
    };

    RegisterClassW(&wnd_class);
}

unsafe fn get_state_ptr(hwnd: HWND) -> *mut PopupWindowState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PopupWindowState
}

fn popup_bottom_right_pos() -> (i32, i32) {
    unsafe {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        let x = screen_w - POPUP_WIDTH as i32 - POPUP_MARGIN_RIGHT;
        let y = screen_h - POPUP_HEIGHT as i32 - POPUP_MARGIN_BOTTOM;

        (x, y)
    }
}

fn popup_window_outer_pos() -> (i32, i32) {
    let (x, y) = popup_bottom_right_pos();
    (x - SHADOW_PAD, y - SHADOW_PAD)
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
