#![windows_subsystem = "windows"]

mod app_config;
mod autostart;
mod config;
mod lastfm;
mod models;
mod server;
mod ui;

use crate::app_config::{default_app_config, load_app_config, save_app_config};
use crate::config::{
    APP_FOOTER_TEXT, TEST_ARTIST_TEXT, TEST_TRACK_TEXT, TRAY_MENU_AUTOSTART_DISABLE_TEXT,
    TRAY_MENU_AUTOSTART_ENABLE_TEXT, TRAY_MENU_STATUS_CONNECTED_TEXT,
    TRAY_MENU_STATUS_NOT_DETECTED_TEXT,
};
use crate::models::{PopupKind, PopupPayload};
use crate::server::{validate_lastfm_credentials, ExtensionStatusNotifier, PopupNotifier};
use crate::ui::tray::build_tray;
use crate::ui::{show_lastfm_settings_window, show_popup_window};

use std::process;
use std::process::Command;
use std::sync::{mpsc, Arc};
use std::thread;
use tray_icon::menu::MenuEvent;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MessageBoxW, PostQuitMessage, TranslateMessage, MB_ICONERROR,
    MB_ICONINFORMATION, MB_OK, MSG,
};

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn show_info_box(title: &str, text: &str) {
    let title_w = to_wide_null(title);
    let text_w = to_wide_null(text);

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
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
            std::ptr::null_mut(),
            text_w.as_ptr(),
            title_w.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn show_install_instructions() {
    show_info_box(
        "Install Chrome Extension",
        "1. Open the Chrome Web Store page.\n\
         2. Click Add to Chrome.\n\
         3. Open music.yandex.ru.\n\
         4. Keep this desktop app running.\n\
         \n\
         After the extension starts sending heartbeat, tray status will change to:\n\
         Chrome extension: connected",
    );
}

fn open_chrome_extensions_page() {
    let url = "https://chromewebstore.google.com/detail/cjemkikpabifhldcdkopinpejahljcin?utm_source=item-share-cb";

    let result = Command::new("cmd").args(["/C", "start", "", url]).spawn();

    if let Err(e) = result {
        show_info_box(
            "Open extension page",
            &format!(
                "Could not open the Chrome Web Store page automatically.\n\n\
                 Open this link manually:\n\
                 {}\n\n\
                 Error: {}",
                url, e
            ),
        );
    }
}

fn refresh_autostart_menu_text(item: &tray_icon::menu::MenuItem, enabled: bool) {
    if enabled {
        item.set_text(TRAY_MENU_AUTOSTART_DISABLE_TEXT);
    } else {
        item.set_text(TRAY_MENU_AUTOSTART_ENABLE_TEXT);
    }
}

fn load_existing_or_default_lastfm_config() -> Result<app_config::AppConfig, String> {
    Ok(app_config::load_app_config()?.unwrap_or_else(app_config::default_app_config))
}

fn has_usable_lastfm_config() -> Result<Option<app_config::AppConfig>, String> {
    let cfg = app_config::load_app_config()?;
    Ok(cfg.filter(|c| c.is_complete()))
}

fn schedule_startup_settings_recovery() {
    thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(4));

        match has_usable_lastfm_config() {
            Ok(Some(_)) => {
                eprintln!("[STARTUP] Last.fm config already available after sync window");
                return;
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("[STARTUP] config recheck error: {}", e);
            }
        }

        let initial_config = load_app_config()
            .ok()
            .flatten()
            .unwrap_or_else(default_app_config);

        match show_lastfm_settings_window(initial_config) {
            Ok(Some(config)) => {
                if let Err(e) = save_app_config(&config) {
                    show_error_box("Save Last.fm settings", &e);
                } else {
                    show_info_box(
                        "Last.fm settings",
                        "Settings saved.\n\nIf the app is already running, manual credentials will be applied after restart.\n\nIf the browser extension is installed, it can also sync Last.fm automatically.",
                    );
                }
            }
            Ok(None) => {}
            Err(e) => {
                show_error_box("Last.fm settings", &e);
            }
        }
    });
}

fn main() {
    let mut app_config = match load_existing_or_default_lastfm_config() {
        Ok(config) => config,
        Err(e) => {
            show_error_box("Last.fm settings", &e);
            process::exit(1);
        }
    };

    if let Err(e) = autostart::sync_autostart(app_config.launch_on_startup) {
        show_error_box(
            "Autostart",
            &format!("Failed to apply autostart setting:\n\n{e}"),
        );
    }

    let popup_notifier: PopupNotifier = Arc::new(|payload: PopupPayload| {
        thread::spawn(move || {
            if let Err(e) = show_popup_window(payload) {
                eprintln!("popup error: {e}");
            }
        });
    });

    let (tray_status_tx, tray_status_rx) = mpsc::channel::<bool>();

    let extension_status_notifier: ExtensionStatusNotifier = Arc::new(move |connected: bool| {
        let _ = tray_status_tx.send(connected);
    });

    let server_popup_notifier = Arc::clone(&popup_notifier);
    let server_extension_status_notifier = Arc::clone(&extension_status_notifier);
    let server_lastfm_config = app_config.lastfm.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        if let Err(e) = rt.block_on(server::run_server(
            server_lastfm_config,
            server_popup_notifier,
            server_extension_status_notifier,
        )) {
            eprintln!("server error: {e}");
        }
    });

    if !app_config.is_complete() {
        schedule_startup_settings_recovery();
    }

    let (
        _tray,
        tray_status_item,
        tray_autostart_item,
        tray_install_id,
        tray_open_extensions_id,
        tray_autostart_id,
        tray_lastfm_settings_id,
        tray_validate_lastfm_id,
        tray_show_id,
        tray_quit_id,
    ) = build_tray();

    refresh_autostart_menu_text(&tray_autostart_item, app_config.launch_on_startup);

    unsafe {
        let mut msg = std::mem::zeroed::<MSG>();

        loop {
            while let Ok(connected) = tray_status_rx.try_recv() {
                if connected {
                    tray_status_item.set_text(TRAY_MENU_STATUS_CONNECTED_TEXT);
                } else {
                    tray_status_item.set_text(TRAY_MENU_STATUS_NOT_DETECTED_TEXT);
                }
            }

            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == tray_install_id {
                    show_install_instructions();
                } else if event.id == tray_open_extensions_id {
                    open_chrome_extensions_page();
                } else if event.id == tray_autostart_id {
                    let new_value = !app_config.launch_on_startup;

                    match autostart::sync_autostart(new_value) {
                        Ok(()) => {
                            app_config.launch_on_startup = new_value;

                            match save_app_config(&app_config) {
                                Ok(()) => {
                                    refresh_autostart_menu_text(
                                        &tray_autostart_item,
                                        app_config.launch_on_startup,
                                    );

                                    show_info_box(
                                        "Autostart",
                                        if new_value {
                                            "Autostart enabled.\n\nThe app will launch after Windows sign-in."
                                        } else {
                                            "Autostart disabled."
                                        },
                                    );
                                }
                                Err(e) => {
                                    show_error_box(
                                        "Autostart",
                                        &format!(
                                            "Autostart changed, but config save failed:\n\n{e}"
                                        ),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            show_error_box(
                                "Autostart",
                                &format!("Failed to change autostart:\n\n{e}"),
                            );
                        }
                    }
                } else if event.id == tray_lastfm_settings_id {
                    let current_config = match load_app_config() {
                        Ok(Some(config)) => config,
                        Ok(None) => default_app_config(),
                        Err(e) => {
                            show_error_box("Load Last.fm settings", &e);
                            continue;
                        }
                    };

                    match show_lastfm_settings_window(current_config) {
                        Ok(Some(new_config)) => match save_app_config(&new_config) {
                            Ok(()) => {
                                app_config = new_config;
                                show_info_box(
                                    "Last.fm settings",
                                    "Settings saved.\n\nRestart the app to apply the new Last.fm credentials.",
                                );
                            }
                            Err(e) => {
                                show_error_box("Save Last.fm settings", &e);
                            }
                        },
                        Ok(None) => {}
                        Err(e) => {
                            show_error_box("Last.fm settings", &e);
                        }
                    }
                } else if event.id == tray_validate_lastfm_id {
                    let config = match load_app_config() {
                        Ok(Some(config)) => config,
                        Ok(None) => {
                            show_error_box(
                                "Validate Last.fm connection",
                                "Last.fm settings are missing.",
                            );
                            continue;
                        }
                        Err(e) => {
                            show_error_box("Load Last.fm settings", &e);
                            continue;
                        }
                    };

                    if !config.is_complete() {
                        show_error_box(
                            "Validate Last.fm connection",
                            "Last.fm settings are incomplete.",
                        );
                        continue;
                    }

                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            show_error_box(
                                "Validate Last.fm connection",
                                &format!("tokio runtime error: {e}"),
                            );
                            continue;
                        }
                    };

                    match rt.block_on(async { validate_lastfm_credentials(&config.lastfm).await }) {
                        Ok(_) => {
                            show_info_box("Validate Last.fm connection", "Connection OK.");
                        }
                        Err(e) => {
                            show_error_box("Validate Last.fm connection", &e);
                        }
                    }
                } else if event.id == tray_show_id {
                    popup_notifier(PopupPayload {
                        kind: PopupKind::Track,
                        title: TEST_ARTIST_TEXT.to_string(),
                        line1: TEST_TRACK_TEXT.to_string(),
                        line2: String::new(),
                        footer: APP_FOOTER_TEXT.to_string(),
                        cover_url: None,
                        cover_path: None,
                        dominant_rgb: None,
                    });
                } else if event.id == tray_quit_id {
                    PostQuitMessage(0);
                }
            }

            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret <= 0 {
                break;
            }

            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    process::exit(0);
}
