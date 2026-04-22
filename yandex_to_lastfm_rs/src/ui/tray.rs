use crate::config::{
    TRAY_MENU_AUTOSTART_ENABLE_TEXT, TRAY_MENU_EXIT_TEXT, TRAY_MENU_INSTALL_TEXT,
    TRAY_MENU_LASTFM_SETTINGS_TEXT, TRAY_MENU_OPEN_EXTENSIONS_TEXT, TRAY_MENU_SHOW_TEXT,
    TRAY_MENU_STATUS_NOT_DETECTED_TEXT, TRAY_MENU_VALIDATE_LASTFM_TEXT, TRAY_TOOLTIP,
};

use tray_icon::{
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

pub fn load_tray_icon() -> Icon {
    let bytes = include_bytes!("../../assets/tray.png");

    let img = image::load_from_memory(bytes)
        .expect("decode tray icon")
        .into_rgba8();

    let (w, h) = img.dimensions();
    let rgba = img.into_raw();

    Icon::from_rgba(rgba, w, h).expect("tray icon rgba")
}

pub fn build_tray() -> (
    TrayIcon,
    MenuItem,
    MenuItem,
    MenuId,
    MenuId,
    MenuId,
    MenuId,
    MenuId,
    MenuId,
    MenuId,
) {
    let menu = Menu::new();

    let status_item = MenuItem::new(TRAY_MENU_STATUS_NOT_DETECTED_TEXT, false, None);
    let install_item = MenuItem::new(TRAY_MENU_INSTALL_TEXT, true, None);
    let open_extensions_item = MenuItem::new(TRAY_MENU_OPEN_EXTENSIONS_TEXT, true, None);
    let autostart_item = MenuItem::new(TRAY_MENU_AUTOSTART_ENABLE_TEXT, true, None);
    let lastfm_settings_item = MenuItem::new(TRAY_MENU_LASTFM_SETTINGS_TEXT, true, None);
    let validate_lastfm_item = MenuItem::new(TRAY_MENU_VALIDATE_LASTFM_TEXT, true, None);
    let show_item = MenuItem::new(TRAY_MENU_SHOW_TEXT, true, None);
    let quit_item = MenuItem::new(TRAY_MENU_EXIT_TEXT, true, None);

    menu.append(&status_item).expect("append status menu");
    menu.append(&PredefinedMenuItem::separator())
        .expect("append separator");
    menu.append(&install_item).expect("append install menu");
    menu.append(&open_extensions_item)
        .expect("append open extensions menu");
    menu.append(&autostart_item).expect("append autostart menu");
    menu.append(&lastfm_settings_item)
        .expect("append lastfm settings menu");
    menu.append(&validate_lastfm_item)
        .expect("append validate lastfm menu");
    menu.append(&show_item).expect("append show menu");
    menu.append(&PredefinedMenuItem::separator())
        .expect("append separator");
    menu.append(&quit_item).expect("append quit menu");

    let install_id = install_item.id().clone();
    let open_extensions_id = open_extensions_item.id().clone();
    let autostart_id = autostart_item.id().clone();
    let lastfm_settings_id = lastfm_settings_item.id().clone();
    let validate_lastfm_id = validate_lastfm_item.id().clone();
    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let tray = TrayIconBuilder::new()
        .with_tooltip(TRAY_TOOLTIP)
        .with_icon(load_tray_icon())
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(true)
        .build()
        .expect("build tray icon");

    (
        tray,
        status_item,
        autostart_item,
        install_id,
        open_extensions_id,
        autostart_id,
        lastfm_settings_id,
        validate_lastfm_id,
        show_id,
        quit_id,
    )
}
