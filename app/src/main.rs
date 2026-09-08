mod config;
mod history;
mod input;
mod keymap;
mod link;
mod protocol;

use config::{Settings, Theme};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Vec2};
use history::{History, XkbHistory};
use input::{HidState, InputCommand, InputEvent};
use link::{LinkCommand, LinkEvent, LinkState};
use std::{
    collections::VecDeque,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn main() -> eframe::Result<()> {
    let settings = Settings::load().unwrap_or_default();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let screenshot_path = args
        .windows(2)
        .find_map(|pair| (pair[0] == "--screenshot").then(|| PathBuf::from(&pair[1])));
    // Bounded native visual checks without changing saved window preferences.
    let screenshot_size = screenshot_path.as_ref().and_then(|_| {
        args.windows(2).find_map(|pair| {
            if pair[0] != "--screenshot-size" {
                return None;
            }
            let (width, height) = pair[1].split_once('x')?;
            Some([
                width.parse::<f32>().ok()?.clamp(460.0, 2560.0),
                height.parse::<f32>().ok()?.clamp(300.0, 1600.0),
            ])
        })
    });
    let screenshot_scale = screenshot_path.as_ref().and_then(|_| {
        args.windows(2).find_map(|pair| {
            if pair[0] != "--screenshot-scale" {
                return None;
            }
            Some(pair[1].parse::<f32>().ok()?.clamp(1.0, 2.0))
        })
    });
    let mut native = eframe::NativeOptions::default();
    native.viewport = egui::ViewportBuilder::default()
        .with_inner_size(screenshot_size.unwrap_or([780.0, 520.0]))
        .with_min_inner_size([460.0, 300.0])
        .with_decorations(true);
    eframe::run_native(
        "Wireless Keyboard Bridge",
        native,
        Box::new(move |cc| {
            Ok(Box::new(BridgeApp::new(
                cc,
                settings,
                screenshot_path,
                screenshot_scale,
            )))
        }),
    )
}

struct BridgeApp {
    settings: Settings,
    input_tx: mpsc::Sender<InputCommand>,
    input_rx: mpsc::Receiver<InputEvent>,
    link_tx: mpsc::Sender<LinkCommand>,
    link_rx: mpsc::Receiver<LinkEvent>,
    capture_gate: Arc<AtomicBool>,
    lease_ms: Arc<AtomicU64>,
    history: History,
    xkb: Option<XkbHistory>,
    hid: HidState,
    link_state: LinkState,
    activation_intent: bool,
    active: bool,
    input_generation: u64,
    capture_session: Option<u64>,
    had_focus: bool,
    error: Option<String>,
    started: Instant,
    app_start_ms: u128,
    usb_ms: Option<u128>,
    a_boot_us: Option<u32>,
    b_boot_us: Option<u32>,
    radio_us: Option<u32>,
    lock_leds: u8,
    latencies: VecDeque<f64>,
    clock_offset_us: Option<f64>,
    best_sync_rtt_us: u64,
    screenshot_path: Option<PathBuf>,
    screenshot_requested: bool,
    screenshot_scale: Option<f32>,
    history_revision: u64,
}

impl BridgeApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        settings: Settings,
        screenshot_path: Option<PathBuf>,
        screenshot_scale: Option<f32>,
    ) -> Self {
        let started = Instant::now();
        let app_start_ms = 0;
        let _ = install_system_font(&cc.egui_ctx);
        let capture_gate = Arc::new(AtomicBool::new(false));
        let lease_ms = Arc::new(AtomicU64::new(0));
        let (input_tx, input_command_rx) = mpsc::channel();
        let (input_event_tx, input_rx) = mpsc::channel();
        input::spawn(
            input_command_rx,
            input_event_tx,
            capture_gate.clone(),
            started,
        );
        let (link_tx, link_command_rx) = mpsc::channel();
        let (link_event_tx, link_rx) = mpsc::channel();
        if settings.cdc_path.is_empty() {
            let _ = link_event_tx.send(LinkEvent::State(LinkState::Error(
                "configure bridge CDC path".into(),
            )));
        } else {
            link::spawn(
                settings.cdc_path.clone(),
                settings.debug,
                link_command_rx,
                link_event_tx,
                capture_gate.clone(),
                lease_ms.clone(),
                started,
            );
        }
        apply_theme(&cc.egui_ctx, &settings.theme);
        Self {
            settings,
            input_tx,
            input_rx,
            link_tx,
            link_rx,
            capture_gate,
            lease_ms,
            history: History::default(),
            xkb: XkbHistory::new().ok(),
            hid: HidState::default(),
            link_state: LinkState::Connecting,
            activation_intent: false,
            active: false,
            input_generation: 0,
            capture_session: None,
            had_focus: false,
            error: None,
            started,
            app_start_ms,
            usb_ms: None,
            a_boot_us: None,
            b_boot_us: None,
            radio_us: None,
            lock_leds: 0,
            latencies: VecDeque::with_capacity(20),
            clock_offset_us: None,
            best_sync_rtt_us: u64::MAX,
            screenshot_path,
            screenshot_requested: false,
            screenshot_scale,
            history_revision: 0,
        }
    }
    fn activate(&mut self) {
        if self.activation_intent || !matches!(self.link_state, LinkState::Connected) {
            return;
        }
        if self.settings.keyboard_path.is_empty() {
            self.error = Some("Error: configure keyboard path".into());
            return;
        }
        self.error = None;
        self.clock_offset_us = None;
        self.best_sync_rtt_us = u64::MAX;
        self.activation_intent = true;
        self.begin_capture();
    }
    fn begin_capture(&mut self) {
        if !self.activation_intent
            || !self.had_focus
            || !matches!(self.link_state, LinkState::Connected)
        {
            return;
        }
        self.input_generation = self.input_generation.wrapping_add(1);
        self.capture_session = None;
        self.capture_gate.store(true, Ordering::Release);
        self.lease_ms
            .store(self.started.elapsed().as_millis() as u64, Ordering::Release);
        let _ = self.link_tx.send(LinkCommand::Begin {
            generation: self.input_generation,
        });
    }
    fn suspend_input(&mut self) {
        self.input_generation = self.input_generation.wrapping_add(1);
        self.capture_gate.store(false, Ordering::Release);
        self.lease_ms.store(0, Ordering::Release);
        let _ = self.input_tx.send(InputCommand::Stop {
            generation: self.input_generation,
        });
        self.active = false;
        self.capture_session = None;
        self.hid = HidState::default();
        self.history.clear_pending_modifiers();
        self.xkb = XkbHistory::new().ok();
    }
    fn stop(&mut self) {
        self.activation_intent = false;
        self.suspend_input();
        let _ = self.link_tx.send(LinkCommand::Stop);
    }
    fn accept_key(&mut self, code: u16, pressed: bool, received_us: u64) {
        if !self.capture_gate.load(Ordering::Acquire) {
            return;
        }
        if pressed {
            if let Some(xkb) = self.xkb.as_mut() {
                xkb.press(&mut self.history, code, self.hid.modifiers);
            }
        } else if let Some(xkb) = self.xkb.as_mut() {
            xkb.release(&mut self.history, code);
        }
        if self.hid.apply(code, pressed) {
            let _ = self.link_tx.send(LinkCommand::State {
                generation: self.input_generation,
                modifiers: self.hid.modifiers,
                bitmap: self.hid.bitmap,
                press: pressed,
                captured_us: received_us,
            });
        }
    }
    fn drain_events(&mut self) {
        while let Ok(event) = self.input_rx.try_recv() {
            match event {
                InputEvent::Key {
                    generation,
                    code,
                    pressed,
                    received_us,
                } if self.active && generation == self.input_generation => {
                    self.accept_key(code, pressed, received_us)
                }
                InputEvent::Fault { generation, detail } if generation == self.input_generation => {
                    self.error = Some(format!("Error: {detail}"));
                    self.stop();
                }
                InputEvent::Started { generation } if generation == self.input_generation => {
                    if self.activation_intent
                        && self.capture_session.is_some()
                        && self.had_focus
                        && self.capture_gate.load(Ordering::Acquire)
                    {
                        self.active = true;
                    } else {
                        self.stop();
                    }
                }
                InputEvent::Stopped { generation } if generation <= self.input_generation => {}
                _ => {}
            }
        }
        while let Ok(event) = self.link_rx.try_recv() {
            match event {
                LinkEvent::State(state) => {
                    if matches!(state, LinkState::Error(_)) {
                        self.error = match &state {
                            LinkState::Error(text) => Some(format!("Error: {text}")),
                            _ => None,
                        };
                        self.stop();
                    }
                    if matches!(state, LinkState::Searching) && self.usb_ms.is_none() {
                        self.usb_ms = Some(self.started.elapsed().as_millis());
                    }
                    self.link_state = state;
                    if matches!(self.link_state, LinkState::TargetUsbUnavailable)
                        && self.activation_intent
                    {
                        self.suspend_input();
                    } else if matches!(self.link_state, LinkState::Connected)
                        && self.activation_intent
                        && !self.capture_gate.load(Ordering::Acquire)
                    {
                        self.begin_capture();
                    }
                }
                LinkEvent::CaptureAuthorized {
                    session,
                    generation,
                } => {
                    if generation != self.input_generation {
                        continue;
                    }
                    if !self.activation_intent
                        || !self.had_focus
                        || !self.capture_gate.load(Ordering::Acquire)
                    {
                        self.stop();
                    } else {
                        self.capture_session = Some(session);
                        self.capture_gate.store(true, Ordering::Release);
                        self.lease_ms
                            .store(self.started.elapsed().as_millis() as u64, Ordering::Release);
                        let _ = self.input_tx.send(InputCommand::Start {
                            path: self.settings.keyboard_path.clone().into(),
                            generation,
                        });
                    }
                }
                LinkEvent::Ack {
                    session,
                    generation,
                    sequence: _sequence,
                    press,
                    captured_us,
                    hid_us,
                } => {
                    if self.capture_session == Some(session)
                        && generation == self.input_generation
                        && self.settings.debug
                        && press
                    {
                        if let (Some(offset), Some(hid)) = (self.clock_offset_us, hid_us) {
                            let value = ((hid as f64 - offset) - captured_us as f64) / 1000.0;
                            if value.is_finite() && value >= 0.0 {
                                if self.latencies.len() == 20 {
                                    self.latencies.pop_front();
                                }
                                self.latencies.push_back(value);
                            }
                        }
                    }
                }
                LinkEvent::Status {
                    a_boot_us,
                    b_boot_us,
                    radio_ready_us,
                } => {
                    self.a_boot_us = Some(a_boot_us);
                    self.b_boot_us = Some(b_boot_us);
                    self.radio_us = Some(radio_ready_us);
                }
                LinkEvent::Leds(value) => self.lock_leds = value,
                LinkEvent::ClockOffset {
                    offset_us,
                    round_trip_us,
                } if self.settings.debug => {
                    if round_trip_us < self.best_sync_rtt_us {
                        self.best_sync_rtt_us = round_trip_us;
                        self.clock_offset_us = Some(offset_us);
                    }
                }
                LinkEvent::ClockOffset { .. } => {}
            }
        }
    }
    fn save_settings(&mut self, ctx: &egui::Context) {
        let _ = self.settings.save();
        apply_theme(ctx, &self.settings.theme);
        let _ = self
            .link_tx
            .send(LinkCommand::SetDebug(self.settings.debug));
        if !self.settings.debug {
            self.latencies.clear();
            self.clock_offset_us = None;
            self.best_sync_rtt_us = u64::MAX;
        }
    }
    fn status(&self) -> (Color32, &'static str, bool) {
        if self.error.is_some() || matches!(self.link_state, LinkState::Error(_)) {
            return (Color32::from_rgb(255, 0, 0), "Error", blink(120, 120));
        }
        if self.active && !self.hid.is_empty() {
            return (Color32::WHITE, "Activity", true);
        }
        match self.link_state {
            LinkState::Connecting => (
                Color32::from_rgb(64, 32, 255),
                "Connecting USB CDC",
                blink(150, 250),
            ),
            LinkState::Searching => {
                if self.started.elapsed() > Duration::from_secs(60) {
                    (
                        Color32::from_rgb(255, 72, 0),
                        "No Connection",
                        blink(400, 1600),
                    )
                } else {
                    (
                        Color32::from_rgb(0, 255, 0),
                        "Connected - Searching",
                        blink(300, 700),
                    )
                }
            }
            LinkState::TargetUsbUnavailable => (
                Color32::from_rgb(255, 150, 0),
                "Target USB unavailable - waiting for reconnect",
                blink(450, 550),
            ),
            LinkState::Connected if self.activation_intent && !self.active => (
                Color32::from_rgb(0, 96, 255),
                "Starting capture",
                blink(300, 300),
            ),
            LinkState::Connected if !self.active => (
                Color32::from_rgb(0, 64, 0),
                "Paused - All devices connected - No keypresses are captured or transmitted",
                true,
            ),
            LinkState::Connected | LinkState::Capturing => {
                (Color32::from_rgb(0, 96, 255), "Connected", true)
            }
            LinkState::Error(_) => (Color32::RED, "Error", blink(120, 120)),
        }
    }
    fn debug_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("App start {} ms", self.app_start_ms)];
        if let Some(value) = self.usb_ms {
            lines.push(format!("USB CDC {value} ms"));
        }
        if let Some(value) = self.a_boot_us {
            lines.push(format!("ESP A boot {} ms", value / 1000));
        }
        if let Some(value) = self.b_boot_us {
            lines.push(format!("ESP B boot {} ms", value / 1000));
        }
        if let Some(value) = self.radio_us {
            lines.push(format!("ESP-NOW {} ms", value / 1000));
        }
        if self.lock_leds != 0 {
            lines.push(format!("Target LEDs 0x{:02x}", self.lock_leds));
        }
        if self.latencies.is_empty() {
            lines.push("Latency unavailable".into());
        } else {
            let min = self.latencies.iter().copied().fold(f64::INFINITY, f64::min);
            let max = self.latencies.iter().copied().fold(0.0, f64::max);
            let avg = self.latencies.iter().sum::<f64>() / self.latencies.len() as f64;
            lines.push(format!(
                "Latency min {min:.1} ms / max {max:.1} ms / avg {avg:.1} ms"
            ));
        }
        if let Some(error) = &self.error {
            lines.push(error.clone());
        }
        lines
    }
}

impl eframe::App for BridgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let (Some(scale), Some(native_scale)) =
            (self.screenshot_scale, ctx.native_pixels_per_point())
        {
            let zoom = scale / native_scale;
            if (ctx.zoom_factor() - zoom).abs() > f32::EPSILON {
                // Wayland provides the monitor scale after creation. Reapply
                // the requested physical scale until the screenshot is taken.
                ctx.set_zoom_factor(zoom);
            }
        }
        let focused = ctx.input(|i| i.focused);
        if self.had_focus && !focused {
            self.stop();
        }
        self.had_focus = focused;
        // The focus lease covers the READY handshake as well as active input.
        if focused && self.capture_gate.load(Ordering::Acquire) {
            self.lease_ms
                .store(self.started.elapsed().as_millis() as u64, Ordering::Release);
        }
        self.drain_events();
        if self.screenshot_path.is_some()
            && !self.screenshot_requested
            && self.started.elapsed() >= Duration::from_millis(300)
        {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        for event in ctx.input(|input| input.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                if let Some(path) = self.screenshot_path.take() {
                    let pixels = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    if let Some(png) = image::RgbaImage::from_raw(
                        image.size[0] as u32,
                        image.size[1] as u32,
                        pixels,
                    ) {
                        let _ = png.save(path);
                    }
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        egui::CentralPanel::default()
            .frame(
                Frame::default().fill(if self.settings.theme == Theme::Dark {
                    Color32::from_rgb(22, 23, 27)
                } else {
                    Color32::from_rgb(244, 245, 247)
                }),
            )
            .show(ctx, |ui| {
                let available = ui.available_rect_before_wrap();
                let surface = Frame::default()
                    .fill(if self.settings.theme == Theme::Dark {
                        Color32::from_rgb(35, 37, 43)
                    } else {
                        Color32::WHITE
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if self.settings.theme == Theme::Dark {
                            Color32::from_gray(65)
                        } else {
                            Color32::from_gray(210)
                        },
                    ))
                    .corner_radius(CornerRadius::same(16))
                    .outer_margin(Margin::same(8))
                    .inner_margin(Margin::same(24));
                surface.show(ui, |ui| {
                    // `available` is the outer panel size. Reserve the frame's
                    // 8 px outer margins and two 24 px inner margins so the
                    // rounded surface fits at the minimum native viewport.
                    ui.set_min_size(available.size() - Vec2::splat(64.0));
                    let content_rect = ui.max_rect();
                    let input_rect = surface.widget_rect(content_rect);
                    if self.settings.debug {
                        for line in self.debug_lines() {
                            ui.label(RichText::new(line).small().color(
                                if self.settings.theme == Theme::Dark {
                                    Color32::from_gray(175)
                                } else {
                                    Color32::from_gray(90)
                                },
                            ));
                        }
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(12.0);
                    }
                    let mut history_rect = ui.available_rect_before_wrap();
                    let fade_height = 24.0;
                    // Keep the rounded surface fixed; move both text limits
                    // inward by one line from the former extended viewport.
                    if self.settings.debug {
                        history_rect.min.y += fade_height;
                    }
                    let history_background = if self.settings.theme == Theme::Dark {
                        Color32::from_rgb(35, 37, 43)
                    } else {
                        Color32::WHITE
                    };
                    let history_changed = self.history.revision() != self.history_revision;
                    let (history_lines, caret_line) = self.history.rendered_lines_with_caret();
                    let history_color = if self.settings.theme == Theme::Dark {
                        Color32::from_rgb(232, 233, 236)
                    } else {
                        Color32::from_rgb(27, 29, 33)
                    };
                    {
                        let mut scroll_rect = history_rect;
                        scroll_rect.max.x += 16.0;
                        let mut history_ui =
                            ui.new_child(egui::UiBuilder::new().max_rect(scroll_rect));
                        let ui = &mut history_ui;
                        ui.set_clip_rect(scroll_rect);
                        egui::ScrollArea::vertical()
                            .id_salt("history-scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_max_width(history_rect.width());
                                ui.set_clip_rect(ui.clip_rect().intersect(history_rect));
                                ui.add_space(fade_height);
                                for (line_index, line) in history_lines.iter().enumerate() {
                                    let response = ui.add(
                                        egui::Label::new(
                                            RichText::new(line).size(18.0).color(history_color),
                                        )
                                        .selectable(false)
                                        .wrap(),
                                    );
                                    if history_changed && line_index == caret_line {
                                        ui.scroll_to_rect(response.rect, Some(egui::Align::Center));
                                    }
                                }
                                ui.add_space(fade_height);
                            });
                    }
                    self.history_revision = self.history.revision();
                    paint_history_fades(
                        ui.painter(),
                        history_rect,
                        fade_height,
                        history_background,
                    );

                    if !self.active {
                        ui.painter().rect_filled(
                            input_rect,
                            CornerRadius::same(16),
                            if self.settings.theme == Theme::Dark {
                                Color32::from_rgba_unmultiplied(20, 20, 24, 180)
                            } else {
                                Color32::from_rgba_unmultiplied(245, 245, 248, 190)
                            },
                        );
                    }
                    if !self.active {
                        let reconnecting = self.activation_intent
                            && matches!(self.link_state, LinkState::TargetUsbUnavailable);
                        let starting = self.activation_intent && !reconnecting;
                        let title = if reconnecting {
                            "Target USB unavailable - waiting for reconnect"
                        } else if starting {
                            "starting capture..."
                        } else {
                            "paused - click to activate"
                        };
                        let detail = if reconnecting {
                            "keypresses are discarded until the target reconnects"
                        } else if starting {
                            "waiting for exclusive keyboard access"
                        } else {
                            "no keypresses are captured or transmitted"
                        };
                        let mut overlay_ui =
                            ui.new_child(egui::UiBuilder::new().max_rect(history_rect));
                        overlay_ui.vertical_centered(|ui| {
                            ui.add_space(history_rect.height() * 0.36);
                            ui.add(
                                egui::Label::new(RichText::new(title).size(22.0).strong())
                                    .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(detail).size(15.0))
                                    .selectable(false),
                            );
                        });
                    }
                    // Register the surface after its children: labels and the
                    // history scroll area must not intercept activation clicks.
                    // Leave the scrollbar's gutter to its own click/drag handler.
                    let mut activation_rect = input_rect;
                    activation_rect.max.x = history_rect.right();
                    let response = ui.interact(
                        activation_rect,
                        ui.id().with("input-surface"),
                        Sense::click(),
                    );
                    if response.clicked() {
                        self.activate();
                    }
                    response.context_menu(|ui| {
                        if self.activation_intent {
                            if ui.button("Pause").clicked() {
                                self.stop();
                                ui.close();
                            }
                        } else if ui.button("Activate").clicked() {
                            self.activate();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Toggle light/dark mode").clicked() {
                            self.settings.theme = if self.settings.theme == Theme::Dark {
                                Theme::Light
                            } else {
                                Theme::Dark
                            };
                            self.save_settings(ctx);
                            ui.close();
                        }
                        if ui.button("Toggle debug on/off").clicked() {
                            self.settings.debug = !self.settings.debug;
                            self.save_settings(ctx);
                            ui.close();
                        }
                        if ui.button("Cancel").clicked() {
                            ui.close();
                        }
                        if ui.button("Exit").clicked() {
                            self.stop();
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    // Anchor the indicator to the fixed content corner, not
                    // the UI bounds expanded by history layout.
                    let (color, tip, visible) = self.status();
                    let status_rect = egui::Rect::from_min_size(
                        content_rect.right_top() + Vec2::new(-24.0, 8.0),
                        Vec2::splat(16.0),
                    );
                    let status = ui.allocate_rect(status_rect, Sense::hover());
                    if visible {
                        ui.painter().circle_filled(status_rect.center(), 7.0, color);
                    }
                    status.on_hover_text(tip);
                });
            });
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop();
        let _ = self.input_tx.send(InputCommand::Shutdown);
        let _ = self.link_tx.send(LinkCommand::Shutdown);
    }
}

fn paint_history_fades(painter: &egui::Painter, rect: egui::Rect, height: f32, color: Color32) {
    let painter = painter.with_clip_rect(rect);
    for (top, bottom, top_color, bottom_color) in [
        (rect.top(), rect.top() + height, color, Color32::TRANSPARENT),
        (
            rect.bottom() - height,
            rect.bottom(),
            Color32::TRANSPARENT,
            color,
        ),
    ] {
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(egui::pos2(rect.left(), top), top_color);
        mesh.colored_vertex(egui::pos2(rect.right(), top), top_color);
        mesh.colored_vertex(egui::pos2(rect.left(), bottom), bottom_color);
        mesh.colored_vertex(egui::pos2(rect.right(), bottom), bottom_color);
        mesh.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
        painter.add(egui::Shape::mesh(mesh));
    }
}

fn blink(on_ms: u128, off_ms: u128) -> bool {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(true, |d| d.as_millis() % (on_ms + off_ms) < on_ms)
}
fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    ctx.set_visuals(if *theme == Theme::Dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
}
fn install_system_font(ctx: &egui::Context) -> Option<String> {
    let output = Command::new("fc-match")
        .args(["-f", "%{file}", "sans-serif"])
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    let bytes = std::fs::read(&path).ok()?;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "desktop-sans".into(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)?
        .insert(0, "desktop-sans".into());
    ctx.set_fonts(fonts);
    Some(path)
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn repeated_clicks_send_one_begin_and_pause_allows_a_new_activation() {
        let settings = Settings {
            keyboard_path: "test-keyboard".into(),
            ..Settings::default()
        };
        let (input_tx, _input_commands) = mpsc::channel();
        let (_input_events, input_rx) = mpsc::channel();
        let (link_tx, link_commands) = mpsc::channel();
        let (link_events, link_rx) = mpsc::channel();
        let capture_gate = Arc::new(AtomicBool::new(false));
        let lease_ms = Arc::new(AtomicU64::new(0));
        let started = Instant::now();
        let app_start_ms = 0;
        let screenshot_path = None;
        let screenshot_scale = None;
        let app = BridgeApp {
            settings,
            input_tx,
            input_rx,
            link_tx,
            link_rx,
            capture_gate,
            lease_ms,
            history: History::default(),
            xkb: XkbHistory::new().ok(),
            hid: HidState::default(),
            link_state: LinkState::Connecting,
            activation_intent: false,
            active: false,
            input_generation: 0,
            capture_session: None,
            had_focus: true,
            error: None,
            started,
            app_start_ms,
            usb_ms: None,
            a_boot_us: None,
            b_boot_us: None,
            radio_us: None,
            lock_leds: 0,
            latencies: VecDeque::with_capacity(20),
            clock_offset_us: None,
            best_sync_rtt_us: u64::MAX,
            screenshot_path,
            screenshot_requested: false,
            screenshot_scale,
            history_revision: 0,
        };
        let mut app = app;
        app.link_state = LinkState::Connected;
        app.activate();
        app.activate();
        assert!(matches!(
            link_commands.try_recv(),
            Ok(LinkCommand::Begin { generation: 1 })
        ));
        assert!(link_commands.try_recv().is_err());
        link_events
            .send(LinkEvent::CaptureAuthorized {
                session: 99,
                generation: 0,
            })
            .unwrap();
        app.drain_events();
        assert!(
            app.activation_intent,
            "stale authorization must not revoke intent"
        );
        assert!(link_commands.try_recv().is_err());
        app.stop();
        assert!(!app.capture_gate.load(Ordering::Acquire));
        assert!(matches!(link_commands.try_recv(), Ok(LinkCommand::Stop)));
        app.activate();
        assert!(matches!(
            link_commands.try_recv(),
            Ok(LinkCommand::Begin { generation: 3 })
        ));
    }
}
