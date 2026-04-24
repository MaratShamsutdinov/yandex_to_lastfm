use crate::app_config::{save_app_config, AppConfig, LastfmConfig};
use crate::server::{
    apply_lastfm_config_hot, finish_lastfm_browser_auth, start_lastfm_browser_auth,
};

use eframe::egui;
use tokio::time::sleep;
use winit::platform::windows::EventLoopBuilderExtWindows;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const SETTINGS_WINDOW_TITLE: &str = "Last.fm account";
const AUTH_WAIT_SECS: u64 = 60;

const WINDOW_W: f32 = 612.0;
const WINDOW_H: f32 = 620.0;

const CONTENT_W: f32 = 566.0;
const CARD_INNER_W: f32 = CONTENT_W - 34.0;

const BUTTON_W: f32 = 278.0;
const BUTTON_H: f32 = 42.0;

enum AuthWorkerEvent {
    Connected { session_key: String },
    Failed { error: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoticeTone {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusTone {
    Connected,
    Disconnected,
    Waiting,
    Error,
}

struct StatusView {
    title: String,
    detail: String,
    tone: StatusTone,
}

struct LastfmSettingsApp {
    initial_config: AppConfig,
    result_config: Arc<Mutex<Option<AppConfig>>>,

    auth_pending: bool,
    auth_deadline: Option<Instant>,
    auth_rx: Option<mpsc::Receiver<AuthWorkerEvent>>,
    auth_cancel_flag: Option<Arc<AtomicBool>>,

    notice: String,
    notice_tone: NoticeTone,

    last_auth_error: Option<String>,
    close_requested: bool,
}

impl LastfmSettingsApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        initial_config: AppConfig,
        result_config: Arc<Mutex<Option<AppConfig>>>,
    ) -> Self {
        configure_egui(&cc.egui_ctx);

        let mut app = Self {
            initial_config: initial_config.normalized(),
            result_config,
            auth_pending: false,
            auth_deadline: None,
            auth_rx: None,
            auth_cancel_flag: None,
            notice: String::new(),
            notice_tone: NoticeTone::Info,
            last_auth_error: None,
            close_requested: false,
        };

        app.set_initial_notice();
        app
    }

    fn set_initial_notice(&mut self) {
        if self.is_connected() {
            self.notice =
                "Connected. You can reconnect to renew access or switch account.".to_string();
            self.notice_tone = NoticeTone::Success;
        } else {
            self.notice =
                "Not connected. Connect Last.fm to enable companion scrobbling.".to_string();
            self.notice_tone = NoticeTone::Info;
        }
    }

    fn is_connected(&self) -> bool {
        self.initial_config.normalized().has_companion_auth()
    }

    fn config_for_browser_auth(&self) -> Result<AppConfig, String> {
        let normalized = self.initial_config.normalized();

        if normalized.lastfm.api_key.trim().is_empty()
            || normalized.lastfm.api_secret.trim().is_empty()
        {
            return Err("Last.fm API Key / API Secret missing".to_string());
        }

        Ok(AppConfig {
            lastfm: LastfmConfig {
                api_key: normalized.lastfm.api_key,
                api_secret: normalized.lastfm.api_secret,
                username: String::new(),
                password: String::new(),
                session_key: normalized.lastfm.session_key,
                synced_from_extension: false,
                auth_token: normalized.lastfm.auth_token,
                auth_token_requested_at: normalized.lastfm.auth_token_requested_at,
            },
            launch_on_startup: normalized.launch_on_startup,
        }
        .normalized())
    }

    fn begin_browser_auth(&mut self) {
        if self.auth_pending {
            return;
        }

        let auth_config = match self.config_for_browser_auth() {
            Ok(config) => config,
            Err(err) => {
                self.last_auth_error = Some(err.clone());
                self.notice = clean_error_message(&err);
                self.notice_tone = NoticeTone::Error;
                return;
            }
        };

        let (tx, rx) = mpsc::channel::<AuthWorkerEvent>();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let worker_cancel_flag = Arc::clone(&cancel_flag);

        self.auth_pending = true;
        self.auth_deadline = Some(Instant::now() + Duration::from_secs(AUTH_WAIT_SECS));
        self.auth_rx = Some(rx);
        self.auth_cancel_flag = Some(cancel_flag);
        self.last_auth_error = None;
        self.notice =
            "Waiting for Last.fm approval. Complete approval in the browser window.".to_string();
        self.notice_tone = NoticeTone::Warning;

        thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .map_err(|e| format!("tokio runtime error: {e}"))
                .and_then(|rt| {
                    rt.block_on(async move {
                        let (token, _auth_url) =
                            start_lastfm_browser_auth(&auth_config.lastfm).await?;

                        wait_for_lastfm_browser_auth(
                            &auth_config.lastfm,
                            &token,
                            worker_cancel_flag,
                        )
                        .await
                    })
                });

            let event = match result {
                Ok(session_key) => AuthWorkerEvent::Connected { session_key },
                Err(error) => AuthWorkerEvent::Failed { error },
            };

            let _ = tx.send(event);
        });
    }

    fn cancel_waiting(&mut self) {
        if !self.auth_pending {
            return;
        }

        self.cancel_auth_worker();

        self.notice =
            "Waiting cancelled. No changes were saved. Click Connect/Reconnect to try again."
                .to_string();
        self.notice_tone = NoticeTone::Warning;
        self.last_auth_error = None;
    }

    fn timeout_waiting(&mut self) {
        if !self.auth_pending {
            return;
        }

        self.cancel_auth_worker();

        let err = format!(
            "Approval timed out after {} seconds. Click Connect/Reconnect to try again.",
            AUTH_WAIT_SECS
        );

        self.last_auth_error = Some(err.clone());
        self.notice = err;
        self.notice_tone = NoticeTone::Error;
    }

    fn cancel_auth_worker(&mut self) {
        if let Some(flag) = self.auth_cancel_flag.take() {
            flag.store(true, Ordering::SeqCst);
        }

        self.auth_pending = false;
        self.auth_deadline = None;
        self.auth_rx = None;
    }

    fn poll_auth_worker(&mut self) {
        let Some(rx) = self.auth_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(AuthWorkerEvent::Connected { session_key }) => {
                self.auth_pending = false;
                self.auth_deadline = None;
                self.auth_cancel_flag = None;
                self.finish_connected(session_key);
            }
            Ok(AuthWorkerEvent::Failed { error }) => {
                self.auth_pending = false;
                self.auth_deadline = None;
                self.auth_cancel_flag = None;

                let clean = clean_error_message(&error);
                self.last_auth_error = Some(clean.clone());
                self.notice = clean;
                self.notice_tone = NoticeTone::Error;
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.auth_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.auth_pending = false;
                self.auth_deadline = None;
                self.auth_cancel_flag = None;

                let err = "Last.fm auth worker stopped unexpectedly.".to_string();
                self.last_auth_error = Some(err.clone());
                self.notice = err;
                self.notice_tone = NoticeTone::Error;
            }
        }
    }

    fn update_auth_deadline(&mut self) {
        if !self.auth_pending {
            return;
        }

        let Some(deadline) = self.auth_deadline else {
            return;
        };

        if Instant::now() >= deadline {
            self.timeout_waiting();
        }
    }

    fn finish_connected(&mut self, session_key: String) {
        let mut config = match self.config_for_browser_auth() {
            Ok(config) => config,
            Err(err) => {
                let clean = clean_error_message(&err);
                self.last_auth_error = Some(clean.clone());
                self.notice = clean;
                self.notice_tone = NoticeTone::Error;
                return;
            }
        };

        let session_key = session_key.trim().to_string();

        if session_key.is_empty() {
            let err = "Last.fm returned an empty session key.".to_string();
            self.last_auth_error = Some(err.clone());
            self.notice = err;
            self.notice_tone = NoticeTone::Error;
            return;
        }

        config.lastfm.session_key = session_key;
        config.lastfm.auth_token.clear();
        config.lastfm.auth_token_requested_at = 0;

        if let Err(err) = save_app_config(&config) {
            let clean = clean_error_message(&err);
            self.last_auth_error = Some(clean.clone());
            self.notice = clean;
            self.notice_tone = NoticeTone::Error;
            return;
        }

        self.initial_config = config.normalized();

        if let Ok(mut result) = self.result_config.lock() {
            *result = Some(self.initial_config.clone());
        }

        match apply_lastfm_config_hot_blocking(&self.initial_config) {
            Ok(()) => {
                self.last_auth_error = None;
                self.notice = "Connected. Session key saved and applied to the running companion."
                    .to_string();
                self.notice_tone = NoticeTone::Success;
            }
            Err(err) => {
                let clean = clean_error_message(&err);
                self.last_auth_error = Some(clean.clone());
                self.notice = format!("Session key was saved, but hot-apply failed: {}", clean);
                self.notice_tone = NoticeTone::Error;
            }
        }
    }

    fn disconnect(&mut self) {
        if self.auth_pending || !self.is_connected() {
            return;
        }

        let mut config = self.initial_config.normalized();

        let preserved_api_key = config.lastfm.api_key.clone();
        let preserved_api_secret = config.lastfm.api_secret.clone();

        config.lastfm = LastfmConfig {
            api_key: preserved_api_key,
            api_secret: preserved_api_secret,
            username: String::new(),
            password: String::new(),
            session_key: String::new(),
            synced_from_extension: false,
            auth_token: String::new(),
            auth_token_requested_at: 0,
        };

        if let Err(err) = save_app_config(&config) {
            let clean = clean_error_message(&err);
            self.last_auth_error = Some(clean.clone());
            self.notice = clean;
            self.notice_tone = NoticeTone::Error;
            return;
        }

        self.initial_config = config.normalized();

        if let Ok(mut result) = self.result_config.lock() {
            *result = Some(self.initial_config.clone());
        }

        match apply_lastfm_config_hot_blocking(&self.initial_config) {
            Ok(()) => {
                self.last_auth_error = None;
                self.notice =
                    "Disconnected. Local Last.fm session was cleared and runtime state updated."
                        .to_string();
                self.notice_tone = NoticeTone::Success;
            }
            Err(err) => {
                let clean = clean_error_message(&err);
                self.last_auth_error = Some(clean.clone());
                self.notice = format!("Session was cleared, but hot-apply failed: {}", clean);
                self.notice_tone = NoticeTone::Error;
            }
        }
    }

    fn status_view(&self) -> StatusView {
        if self.auth_pending {
            return StatusView {
                title: "Waiting for approval".to_string(),
                detail: format!(
                    "Approve access in the browser. WinApp will finish connection automatically. {}s left.",
                    self.remaining_secs()
                ),
                tone: StatusTone::Waiting,
            };
        }

        if let Some(err) = self.last_auth_error.as_ref() {
            return StatusView {
                title: "Error".to_string(),
                detail: err.clone(),
                tone: StatusTone::Error,
            };
        }

        if self.is_connected() {
            return StatusView {
                title: "Connected".to_string(),
                detail: "Last.fm account is connected through a saved session key.".to_string(),
                tone: StatusTone::Connected,
            };
        }

        StatusView {
            title: "Not connected".to_string(),
            detail: "No Last.fm session key is saved in WinApp.".to_string(),
            tone: StatusTone::Disconnected,
        }
    }

    fn remaining_secs(&self) -> u64 {
        let Some(deadline) = self.auth_deadline else {
            return 0;
        };

        deadline
            .saturating_duration_since(Instant::now())
            .as_secs()
            .min(AUTH_WAIT_SECS)
    }

    fn request_close(&mut self, ctx: &egui::Context) {
        if self.auth_pending {
            self.cancel_waiting();
        }

        self.close_requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn handle_native_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());

        if close_requested && self.auth_pending && !self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close(ctx);
        }
    }

    fn draw(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(bg_color())
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                paint_background(ui);

                ui.add_space(20.0);

                egui::Frame::new()
                    .inner_margin(egui::Margin::same(22))
                    .show(ui, |ui| {
                        self.draw_header(ui);
                        ui.add_space(18.0);

                        self.draw_status_card(ui);
                        ui.add_space(12.0);

                        self.draw_help_card(ui);
                        ui.add_space(16.0);

                        self.draw_actions(ui, ctx);
                        ui.add_space(12.0);

                        self.draw_notice(ui);
                    });
            });
    }

    fn draw_header(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Yandex Music -> Last.fm")
                    .size(28.0)
                    .strong()
                    .color(text_color()),
            );

            ui.add_space(4.0);

            ui.label(
                egui::RichText::new(
                    "Desktop companion account connection. No password is entered in WinApp.",
                )
                .size(14.0)
                .color(mut_color()),
            );
        });
    }

    fn draw_status_card(&self, ui: &mut egui::Ui) {
        let status = self.status_view();
        let accent = status_color(status.tone);

        ui.set_width(CONTENT_W);

        card_frame(accent).show(ui, |ui| {
            ui.set_width(CARD_INNER_W);

            ui.horizontal(|ui| {
                chip_frame(accent).show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(status_icon(status.tone))
                            .size(19.0)
                            .strong()
                            .color(accent),
                    );
                });

                ui.add_space(10.0);

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(status.title)
                            .size(20.0)
                            .strong()
                            .color(text_color()),
                    );

                    ui.add_space(5.0);

                    ui.label(
                        egui::RichText::new(status.detail)
                            .size(13.0)
                            .color(mut_color()),
                    );
                });
            });
        });
    }

    fn draw_help_card(&self, ui: &mut egui::Ui) {
        ui.set_width(CONTENT_W);

        card_frame(accent_color()).show(ui, |ui| {
            ui.set_width(CARD_INNER_W);

            ui.label(
                egui::RichText::new("How connection works")
                    .size(16.0)
                    .strong()
                    .color(text_color()),
            );

            ui.add_space(10.0);

            help_row(ui, "1", "Click Connect/Reconnect");
            help_row(ui, "2", "Your browser will open the Last.fm approval page");
            help_row(
                ui,
                "3",
                "Approve access — WinApp will finish connection automatically",
            );
        });
    }

    fn draw_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let connected = self.is_connected();

        ui.horizontal(|ui| {
            let connect_text = if connected {
                "Reconnect Last.fm"
            } else {
                "Connect Last.fm"
            };

            let connect_enabled = !self.auth_pending;
            if action_button(ui, connect_text, connect_enabled, accent_color()).clicked() {
                self.begin_browser_auth();
            }

            let cancel_enabled = self.auth_pending;
            if action_button(ui, "Cancel waiting", cancel_enabled, warn_color()).clicked() {
                self.cancel_waiting();
            }
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let disconnect_enabled = connected && !self.auth_pending;
            if action_button(ui, "Disconnect", disconnect_enabled, danger_color()).clicked() {
                self.disconnect();
            }

            let close_enabled = true;
            if action_button(ui, "Close", close_enabled, neutral_button_color()).clicked() {
                self.request_close(ctx);
            }
        });
    }

    fn draw_notice(&self, ui: &mut egui::Ui) {
        let color = notice_color(self.notice_tone);

        ui.set_width(CONTENT_W);

        egui::Frame::new()
            .fill(tint(color, 0.08))
            .stroke(egui::Stroke::new(1.0, tint(color, 0.22)))
            .corner_radius(egui::CornerRadius::same(14))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(CONTENT_W - 22.0);

                ui.label(
                    egui::RichText::new(&self.notice)
                        .size(12.0)
                        .color(tint(color, 0.95)),
                );
            });
    }
}

impl Drop for LastfmSettingsApp {
    fn drop(&mut self) {
        if let Some(flag) = self.auth_cancel_flag.take() {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

impl eframe::App for LastfmSettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_native_close_request(ctx);
        self.poll_auth_worker();
        self.update_auth_deadline();
        self.draw(ctx);

        if self.auth_pending {
            ctx.request_repaint_after(Duration::from_millis(200));
        } else {
            ctx.request_repaint_after(Duration::from_millis(700));
        }
    }
}

pub fn show_lastfm_settings_window(initial_config: AppConfig) -> Result<Option<AppConfig>, String> {
    let result_config = Arc::new(Mutex::new(None::<AppConfig>));
    let result_for_app = Arc::clone(&result_config);

    let native_options = eframe::NativeOptions {
        viewport: {
            let mut viewport = egui::ViewportBuilder::default()
                .with_title(SETTINGS_WINDOW_TITLE)
                .with_inner_size(egui::vec2(WINDOW_W, WINDOW_H))
                .with_min_inner_size(egui::vec2(WINDOW_W, WINDOW_H))
                .with_resizable(false);

            if let Some(icon) = load_window_icon() {
                viewport = viewport.with_icon(Arc::new(icon));
            }

            viewport
        },
        centered: true,
        event_loop_builder: Some(Box::new(|builder| {
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };

    eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        native_options,
        Box::new(move |cc| {
            let app: Box<dyn eframe::App> =
                Box::new(LastfmSettingsApp::new(cc, initial_config, result_for_app));

            Ok::<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>>(app)
        }),
    )
    .map_err(|e| format!("settings window error: {e}"))?;

    let result = result_config
        .lock()
        .map(|guard| (*guard).clone())
        .unwrap_or(None);

    Ok(result)
}

async fn wait_for_lastfm_browser_auth(
    lastfm_config: &LastfmConfig,
    token: &str,
    cancel_flag: Arc<AtomicBool>,
) -> Result<String, String> {
    for _ in 0..AUTH_WAIT_SECS {
        if cancel_flag.load(Ordering::SeqCst) {
            return Err("Last.fm approval was cancelled.".to_string());
        }

        match finish_lastfm_browser_auth(lastfm_config, token).await {
            Ok(session_key) => return Ok(session_key),
            Err(err) if is_lastfm_auth_waiting_error(&err) => {
                sleep(Duration::from_secs(1)).await;
            }
            Err(err) => return Err(err),
        }
    }

    Err(format!(
        "Approval timed out after {} seconds. Click Connect/Reconnect to try again.",
        AUTH_WAIT_SECS
    ))
}

fn load_window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/app_128.png");
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();

    Some(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

fn is_lastfm_auth_waiting_error(err: &str) -> bool {
    let e = err.to_lowercase();

    e.contains("unauthorized token")
        || e.contains("\"error\":14")
        || e.contains("\"error\": 14")
        || e.contains("error 14")
}

fn apply_lastfm_config_hot_blocking(config: &AppConfig) -> Result<(), String> {
    let config = config.clone();

    tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime error: {e}"))?
        .block_on(async move { apply_lastfm_config_hot(&config).await })
}

fn clean_error_message(err: &str) -> String {
    let text = err
        .replace("Error: ", "")
        .replace("error: ", "")
        .trim()
        .to_string();

    if text.is_empty() {
        "Unknown error".to_string()
    } else {
        text
    }
}

fn configure_egui(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals.dark_mode = true;
    style.visuals.window_fill = bg_color();
    style.visuals.panel_fill = bg_color();
    style.visuals.override_text_color = Some(text_color());

    style.visuals.widgets.inactive.bg_fill = neutral_button_color();
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, text_color());
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(64, 74, 88);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, text_color());
    style.visuals.widgets.active.bg_fill = accent_color();
    style.visuals.widgets.noninteractive.bg_fill = card_color();

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);

    ctx.set_style(style);
}

fn paint_background(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();

    painter.rect_filled(rect, egui::CornerRadius::same(0), bg_color());

    let glow_1 = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(-90.0, -120.0),
        egui::vec2(360.0, 260.0),
    );

    painter.rect_filled(
        glow_1,
        egui::CornerRadius::same(120),
        egui::Color32::from_rgba_unmultiplied(45, 126, 247, 26),
    );

    let glow_2 = egui::Rect::from_min_size(
        rect.right_top() + egui::vec2(-250.0, 50.0),
        egui::vec2(320.0, 260.0),
    );

    painter.rect_filled(
        glow_2,
        egui::CornerRadius::same(120),
        egui::Color32::from_rgba_unmultiplied(126, 231, 135, 14),
    );
}

fn card_frame(accent: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(card_color())
        .stroke(egui::Stroke::new(1.0, tint(accent, 0.25)))
        .corner_radius(egui::CornerRadius::same(18))
        .inner_margin(egui::Margin::same(16))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 70),
        })
}

fn chip_frame(accent: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(tint(accent, 0.12))
        .stroke(egui::Stroke::new(1.0, tint(accent, 0.30)))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(10))
}

fn help_row(ui: &mut egui::Ui, index: &str, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 26.0), egui::Sense::hover());

        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(9),
            egui::Color32::from_rgb(31, 38, 48),
        );

        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(9),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(54, 64, 78)),
            egui::StrokeKind::Inside,
        );

        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            index,
            egui::FontId::proportional(12.0),
            accent_color(),
        );

        ui.add_space(8.0);

        ui.label(egui::RichText::new(text).size(13.0).color(text_color()));
    });
}

fn action_button(
    ui: &mut egui::Ui,
    text: &str,
    enabled: bool,
    fill: egui::Color32,
) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .size(14.0)
            .strong()
            .color(text_color()),
    )
    .fill(if enabled {
        fill
    } else {
        egui::Color32::from_rgb(40, 45, 52)
    })
    .stroke(egui::Stroke::new(
        1.0,
        if enabled {
            tint(fill, 0.55)
        } else {
            egui::Color32::from_rgb(54, 60, 68)
        },
    ))
    .corner_radius(egui::CornerRadius::same(13))
    .min_size(egui::vec2(BUTTON_W, BUTTON_H));

    ui.add_enabled(enabled, button)
}

fn status_icon(tone: StatusTone) -> &'static str {
    match tone {
        StatusTone::Connected => "✓",
        StatusTone::Disconnected => "•",
        StatusTone::Waiting => "…",
        StatusTone::Error => "!",
    }
}

// fn status_pill_text(tone: StatusTone) -> &'static str {
//     match tone {
//         StatusTone::Connected => "green",
//         StatusTone::Disconnected => "gray",
//         StatusTone::Waiting => "yellow",
//         StatusTone::Error => "red",
//     }
// }

fn status_color(tone: StatusTone) -> egui::Color32 {
    match tone {
        StatusTone::Connected => ok_color(),
        StatusTone::Disconnected => muted_status_color(),
        StatusTone::Waiting => warn_color(),
        StatusTone::Error => danger_color(),
    }
}

fn notice_color(tone: NoticeTone) -> egui::Color32 {
    match tone {
        NoticeTone::Info => accent_color(),
        NoticeTone::Success => ok_color(),
        NoticeTone::Warning => warn_color(),
        NoticeTone::Error => danger_color(),
    }
}

fn tint(color: egui::Color32, alpha: f32) -> egui::Color32 {
    let alpha = alpha.clamp(0.0, 1.0);
    egui::Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (255.0 * alpha).round() as u8,
    )
}

fn bg_color() -> egui::Color32 {
    egui::Color32::from_rgb(15, 17, 22)
}

fn card_color() -> egui::Color32 {
    egui::Color32::from_rgb(23, 28, 35)
}

fn text_color() -> egui::Color32 {
    egui::Color32::from_rgb(243, 245, 247)
}

fn mut_color() -> egui::Color32 {
    egui::Color32::from_rgb(151, 163, 175)
}

fn accent_color() -> egui::Color32 {
    egui::Color32::from_rgb(45, 126, 247)
}

fn neutral_button_color() -> egui::Color32 {
    egui::Color32::from_rgb(57, 66, 76)
}

fn ok_color() -> egui::Color32 {
    egui::Color32::from_rgb(126, 231, 135)
}

fn warn_color() -> egui::Color32 {
    egui::Color32::from_rgb(227, 179, 65)
}

fn danger_color() -> egui::Color32 {
    egui::Color32::from_rgb(255, 123, 114)
}

fn muted_status_color() -> egui::Color32 {
    egui::Color32::from_rgb(156, 166, 178)
}
