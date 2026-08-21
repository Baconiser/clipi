use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Margin,
    PointerButton, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, TextureHandle,
    TextureOptions, Ui, Vec2, ViewportCommand,
};
use eframe::{App, CreationContext};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
#[cfg(not(target_os = "macos"))]
use tray_icon::{MouseButton, MouseButtonState};

use crate::clipboard::{self, WatcherCmd};
use crate::config::Settings;
use crate::hotkey;
use crate::macos;
use crate::store::{ClipBody, ClipKind, ListRow, Store};
use crate::tray::{self, TrayHandles};
use crate::win;

const PALETTE_SIZE: Vec2 = Vec2::new(440.0, 560.0);
const RECENT_LIMIT: u32 = 80;
const SEARCH_LIMIT: u32 = 200;
const TEXTURE_CACHE: usize = 64;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(30);

const PAPER: Color32 = Color32::from_rgb(0x1C, 0x19, 0x16);
const INK: Color32 = Color32::from_rgb(0xE8, 0xE0, 0xD4);
const MUTE: Color32 = Color32::from_rgb(0x8A, 0x80, 0x74);
const RULE: Color32 = Color32::from_rgb(0x3A, 0x34, 0x2C);
const MARK: Color32 = Color32::from_rgb(0xC4, 0x5C, 0x26);
const FIELD: Color32 = Color32::from_rgb(0x14, 0x12, 0x10);
const ROW_HOVER: Color32 = Color32::from_rgb(0x26, 0x22, 0x1C);

#[derive(Clone, Copy)]
enum UiCmd {
    Toggle,
    Show,
    Settings,
    Quit,
}

pub struct ClipiApp {
    settings: Arc<RwLock<Settings>>,
    store: Store,
    watcher_tx: Sender<WatcherCmd>,
    hotkeys: GlobalHotKeyManager,
    registered_hotkey: Option<HotKey>,
    hotkey_capture: bool,
    _tray: TrayHandles,
    query: String,
    last_search: String,
    search_dirty_at: Option<Instant>,
    results: Vec<ListRow>,
    selected: usize,
    visible: bool,
    boot_frames: u8,
    just_opened: bool,
    palette_had_focus: bool,
    quitting: bool,
    settings_open: bool,
    settings_draft: Settings,
    settings_error: Option<String>,
    textures: HashMap<i64, TextureHandle>,
    ui_cmds: Arc<Mutex<Vec<UiCmd>>>,
    win: win::Handle,
    last_pos: Option<Pos2>,
    place_frames: u8,
}

impl ClipiApp {
    pub fn new(cc: &CreationContext<'_>, settings: Settings) -> Result<Self, String> {
        apply_theme(&cc.egui_ctx);
        let settings = Arc::new(RwLock::new(settings));
        let db_path = settings.read().unwrap().db_path.clone();
        let store = Store::open(&db_path)?;
        let watcher_tx = clipboard::start(Arc::clone(&settings), cc.egui_ctx.clone())?;

        let hotkeys = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let registered_hotkey = {
            let raw = settings.read().unwrap().hotkey.clone();
            match hotkey::register(&hotkeys, &raw) {
                Ok(hk) => Some(hk),
                Err(err) => {
                    eprintln!("clipi: hotkey {raw} failed: {err}");
                    None
                }
            }
        };

        let tray = tray::build()?;
        let show_id = tray.show_id.clone();
        let settings_id = tray.settings_id.clone();
        let quit_id = tray.quit_id.clone();
        let ui_cmds = Arc::new(Mutex::new(Vec::new()));
        let win = win::Handle::new();
        win.capture(cc);

        // Handlers must not call AppKit. On Windows they must ShowWindow here
        // so the hidden eframe window wakes and can take foreground.
        let cmds = Arc::clone(&ui_cmds);
        let win_h = win.clone();
        let ctx = cc.egui_ctx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed {
                if let Ok(mut q) = cmds.lock() {
                    q.push(UiCmd::Toggle);
                }
                if !win_h.is_shown() {
                    wake_window(&ctx, &win_h);
                } else {
                    ctx.request_repaint();
                }
            }
        }));
        let cmds = Arc::clone(&ui_cmds);
        let win_h = win.clone();
        let ctx = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let cmd = if event.id == show_id {
                Some(UiCmd::Show)
            } else if event.id == settings_id {
                Some(UiCmd::Settings)
            } else if event.id == quit_id {
                Some(UiCmd::Quit)
            } else {
                None
            };
            match cmd {
                Some(UiCmd::Quit) => tray::force_exit(),
                Some(other) => {
                    if let Ok(mut q) = cmds.lock() {
                        q.push(other);
                    }
                    wake_window(&ctx, &win_h);
                }
                None => ctx.request_repaint(),
            }
        }));
        let cmds = Arc::clone(&ui_cmds);
        let win_h = win.clone();
        let ctx = cc.egui_ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            #[cfg(not(target_os = "macos"))]
            {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    if let Ok(mut q) = cmds.lock() {
                        q.push(UiCmd::Show);
                    }
                    wake_window(&ctx, &win_h);
                    return;
                }
            }
            #[cfg(target_os = "macos")]
            {
                let _ = (&event, &cmds, &win_h);
            }
            ctx.request_repaint();
        }));

        let mut app = Self {
            settings,
            store,
            watcher_tx,
            hotkeys,
            registered_hotkey,
            hotkey_capture: false,
            _tray: tray,
            query: String::new(),
            last_search: String::new(),
            search_dirty_at: None,
            results: Vec::new(),
            selected: 0,
            visible: false,
            boot_frames: 0,
            just_opened: false,
            palette_had_focus: false,
            quitting: false,
            settings_open: false,
            settings_draft: Settings::default(),
            settings_error: None,
            textures: HashMap::new(),
            ui_cmds,
            win,
            last_pos: None,
            place_frames: 0,
        };
        app.last_pos = app.load_palette_pos();
        app.reload_results();
        Ok(app)
    }

    fn show_palette(&mut self, ctx: &egui::Context) {
        self.settings_open = false;
        self.visible = true;
        self.just_opened = true;
        self.palette_had_focus = false;
        self.query.clear();
        self.last_search.clear();
        self.search_dirty_at = None;
        self.selected = 0;
        self.reload_results();
        macos::bring_to_front();
        self.win.show();
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(PALETTE_SIZE));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(ViewportCommand::Title("clipi".into()));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        if self.last_pos.is_none() {
            self.last_pos = self.load_palette_pos();
        }
        self.place_frames = 8;
        place_palette(ctx, PALETTE_SIZE, self.last_pos);
    }

    fn hide_palette(&mut self, ctx: &egui::Context) {
        if self.hotkey_capture {
            self.stop_hotkey_capture();
        }
        if self.visible && !self.settings_open {
            self.remember_palette_pos(ctx);
        }
        self.visible = false;
        self.settings_open = false;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        self.win.hide();
        if self.boot_frames >= 3 {
            macos::resign_accessory();
        }
    }

    fn toggle_palette(&mut self, ctx: &egui::Context) {
        if self.visible && !self.settings_open {
            self.hide_palette(ctx);
        } else {
            self.show_palette(ctx);
        }
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        if let Ok(s) = self.settings.read() {
            self.settings_draft = s.clone();
        }
        self.settings_error = None;
        self.hotkey_capture = false;
        self.settings_open = true;
        self.visible = true;
        macos::bring_to_front();
        self.win.show();
        let size = Vec2::new(520.0, 360.0);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd(ViewportCommand::Title("clipi settings".into()));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        center_on_screen(ctx, size);
    }

    fn reload_results(&mut self) {
        let query = self.query.trim().to_string();
        self.last_search = self.query.clone();
        self.search_dirty_at = None;
        let result = if query.is_empty() {
            self.store.list_recent(RECENT_LIMIT)
        } else if query.eq_ignore_ascii_case("image") {
            self.store.list_images(RECENT_LIMIT)
        } else if let Some(match_q) = fts_query(&query) {
            match self.store.search_fts(&match_q, SEARCH_LIMIT) {
                Ok(rows) => Ok(rank_fuzzy(rows, &query)),
                Err(err) => {
                    eprintln!("clipi: search failed: {err}");
                    self.store.list_recent(RECENT_LIMIT)
                }
            }
        } else {
            self.store.list_recent(RECENT_LIMIT)
        };
        match result {
            Ok(rows) => self.results = rows,
            Err(err) => {
                eprintln!("clipi: list failed: {err}");
                self.results.clear();
            }
        }
        if self.selected >= self.results.len() {
            self.selected = self.results.len().saturating_sub(1);
        }
    }

    fn restore_selected(&mut self, ctx: &egui::Context) {
        let Some(row) = self.results.get(self.selected) else {
            return;
        };
        let id = row.id;
        match self.store.get_body(id) {
            Ok(Some(ClipBody::Text(text))) => {
                let _ = self.watcher_tx.send(WatcherCmd::RestoreText(text));
            }
            Ok(Some(ClipBody::ImagePng(png))) => match clipboard::decode_png_rgba(&png) {
                Ok((width, height, rgba)) => {
                    let _ = self
                        .watcher_tx
                        .send(WatcherCmd::RestoreImage {
                            width,
                            height,
                            rgba,
                        });
                }
                Err(err) => eprintln!("clipi: decode image failed: {err}"),
            },
            Ok(None) => {}
            Err(err) => eprintln!("clipi: load clip failed: {err}"),
        }
        self.hide_palette(ctx);
    }

    fn delete_selected(&mut self) {
        let Some(row) = self.results.get(self.selected) else {
            return;
        };
        let id = row.id;
        if let Err(err) = self.store.delete(id) {
            eprintln!("clipi: delete failed: {err}");
            return;
        }
        self.textures.remove(&id);
        self.reload_results();
    }

    fn poll_events(&mut self, ctx: &egui::Context) {
        let queued = self
            .ui_cmds
            .lock()
            .map(|mut q| q.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        for cmd in queued {
            match cmd {
                UiCmd::Toggle => {
                    if !self.hotkey_capture {
                        self.toggle_palette(ctx);
                    }
                }
                UiCmd::Show => self.show_palette(ctx),
                UiCmd::Settings => self.open_settings(ctx),
                UiCmd::Quit => tray::force_exit(),
            }
        }
    }

    fn save_settings(&mut self, ctx: &egui::Context) {
        self.settings_draft.sanitize();
        let new_path = self.settings_draft.db_path.clone();
        if new_path.as_os_str().is_empty() {
            self.settings_error = Some("Database path is empty.".into());
            return;
        }
        if self.hotkey_capture {
            self.hotkey_capture = false;
        }
        let hotkey_raw = self.settings_draft.hotkey.clone();
        if let Err(err) = self.apply_hotkey(&hotkey_raw) {
            self.settings_error = Some(format!("Could not use that hotkey: {err}"));
            return;
        }
        if let Ok(s) = self.settings.read() {
            self.settings_draft.palette_x = s.palette_x;
            self.settings_draft.palette_y = s.palette_y;
        }
        let old_path = self
            .settings
            .read()
            .map(|s| s.db_path.clone())
            .unwrap_or_default();
        if new_path != old_path {
            match Store::open(&new_path) {
                Ok(store) => self.store = store,
                Err(err) => {
                    self.settings_error = Some(format!("Could not open database: {err}"));
                    return;
                }
            }
        }
        if let Err(err) = self.settings_draft.save() {
            self.settings_error = Some(format!("Could not save settings: {err}"));
            return;
        }
        if let Ok(mut guard) = self.settings.write() {
            *guard = self.settings_draft.clone();
        }
        if new_path != old_path {
            let _ = self.watcher_tx.send(WatcherCmd::Reopen);
        }
        if let Ok(s) = self.settings.read() {
            if let Err(err) = self.store.evict(&s) {
                eprintln!("clipi: evict failed: {err}");
            }
        }
        self.textures.clear();
        self.reload_results();
        self.hide_palette(ctx);
        self.settings_error = None;
    }

    fn start_hotkey_capture(&mut self) {
        self.hotkey_capture = true;
        if let Some(hk) = self.registered_hotkey.take() {
            hotkey::unregister(&self.hotkeys, hk);
        }
    }

    fn stop_hotkey_capture(&mut self) {
        self.hotkey_capture = false;
        let raw = self
            .settings
            .read()
            .ok()
            .map(|s| s.hotkey.clone())
            .unwrap_or_else(|| hotkey::DEFAULT.to_string());
        if let Err(err) = self.apply_hotkey(&raw) {
            eprintln!("clipi: restore hotkey failed: {err}");
        }
    }

    fn apply_hotkey(&mut self, raw: &str) -> Result<(), String> {
        let next = hotkey::parse(raw)?;
        if self.registered_hotkey == Some(next) {
            return Ok(());
        }
        let previous = self.registered_hotkey;
        if let Some(old) = previous {
            hotkey::unregister(&self.hotkeys, old);
            self.registered_hotkey = None;
        }
        match self.hotkeys.register(next) {
            Ok(()) => {
                self.registered_hotkey = Some(next);
                Ok(())
            }
            Err(err) => {
                if let Some(old) = previous {
                    if self.hotkeys.register(old).is_ok() {
                        self.registered_hotkey = Some(old);
                    }
                }
                Err(err.to_string())
            }
        }
    }

    fn load_palette_pos(&self) -> Option<Pos2> {
        if let Ok(Some((x, y))) = self.store.palette_pos() {
            if x.abs() > 1.0 || y.abs() > 1.0 {
                return Some(Pos2::new(x, y));
            }
        }
        let from_config = self.settings.read().ok().and_then(|s| match (s.palette_x, s.palette_y) {
            (Some(x), Some(y)) if x.abs() > 1.0 || y.abs() > 1.0 => Some(Pos2::new(x, y)),
            _ => None,
        })?;
        if let Err(err) = self.store.set_palette_pos(from_config.x, from_config.y) {
            eprintln!("clipi: migrate palette position failed: {err}");
        }
        Some(from_config)
    }

    fn remember_palette_pos(&self, ctx: &egui::Context) {
        let pos = self.last_pos.or_else(|| {
            ctx.input(|i| i.viewport().outer_rect.filter(|r| r.width() > 32.0 && r.height() > 32.0))
                .map(|r| r.min)
        });
        let Some(pos) = pos else {
            return;
        };
        let x = pos.x;
        let y = pos.y;
        let unchanged_db = matches!(self.store.palette_pos(), Ok(Some((ox, oy))) if ox == x && oy == y);
        if !unchanged_db {
            if let Err(err) = self.store.set_palette_pos(x, y) {
                eprintln!("clipi: save palette position failed: {err}");
            }
        }
        let Ok(mut settings) = self.settings.write() else {
            return;
        };
        if settings.palette_x == Some(x) && settings.palette_y == Some(y) {
            return;
        }
        settings.palette_x = Some(x);
        settings.palette_y = Some(y);
        if let Err(err) = settings.save() {
            eprintln!("clipi: save palette position failed: {err}");
        }
    }

    fn track_palette_pos(&mut self, ctx: &egui::Context) {
        if !self.visible || self.settings_open {
            return;
        }
        if self.place_frames > 0 {
            if let Some(pos) = self.last_pos {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
            }
            self.place_frames -= 1;
            ctx.request_repaint();
            return;
        }
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            if rect.width() > 32.0 && rect.height() > 32.0 {
                self.last_pos = Some(rect.min);
            }
        }
    }
}

impl App for ClipiApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        PAPER.to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.win.capture(frame);
        apply_theme(ctx);
        self.poll_events(ctx);
        self.track_palette_pos(ctx);

        if self.boot_frames < 3 {
            self.boot_frames += 1;
            if !self.visible && !self.settings_open {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            }
            ctx.request_repaint();
            if self.boot_frames == 3 && !self.visible && !self.settings_open {
                macos::set_accessory();
            }
        }

        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.quitting && !self.win.is_exiting() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_palette(ctx);
        }

        if self.visible && !self.settings_open && self.boot_frames >= 3 && !self.quitting {
            let focused = ctx.input(|i| i.viewport().focused.unwrap_or(i.focused));
            if focused {
                self.palette_had_focus = true;
            } else if self.palette_had_focus {
                self.hide_palette(ctx);
            }
        }

        if let Some(at) = self.search_dirty_at {
            if at.elapsed() >= SEARCH_DEBOUNCE && self.query != self.last_search {
                self.reload_results();
            } else {
                ctx.request_repaint_after(SEARCH_DEBOUNCE);
            }
        }

        if self.settings_open {
            draw_settings(self, ctx);
        } else if self.visible {
            draw_palette(self, ctx);
        }

        if !self.visible && !self.settings_open {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

fn draw_palette(app: &mut ClipiApp, ctx: &egui::Context) {
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.hide_palette(ctx);
        return;
    }
    if ctx.input(|i| i.key_pressed(Key::Enter)) {
        app.restore_selected(ctx);
        return;
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        if !app.results.is_empty() {
            app.selected = (app.selected + 1).min(app.results.len() - 1);
        }
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        app.selected = app.selected.saturating_sub(1);
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
    }
    if ctx.input(|i| i.key_pressed(Key::Delete)) {
        app.delete_selected();
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Delete));
    }

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(PAPER).inner_margin(Margin::same(12)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
            if draw_move_handle(ui, ctx) {
                app.remember_palette_pos(ctx);
            }
            let search_id = Id::new("clipi-search");
            if app.just_opened {
                ui.memory_mut(|m| m.request_focus(search_id));
                app.just_opened = false;
            }
            let before = app.query.clone();
            let response = ui.add(
                TextEdit::singleline(&mut app.query)
                    .id(search_id)
                    .hint_text("search")
                    .font(FontId::proportional(16.0))
                    .desired_width(f32::INFINITY)
                    .frame(false)
                    .text_color(INK),
            );
            if response.changed() || before != app.query {
                app.search_dirty_at = Some(Instant::now());
                app.selected = 0;
            }
            ui.add_space(4.0);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().top(),
                Stroke::new(1.0, RULE),
            );
            ui.add_space(8.0);

            if app.results.is_empty() {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("Copy something. It will show up here.")
                        .color(MUTE)
                        .size(13.0),
                );
                return;
            }

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    let mut clicked = None;
                    for (idx, row) in app.results.iter().enumerate() {
                        let selected = idx == app.selected;
                        if draw_row(ui, ctx, &mut app.textures, row, selected) {
                            clicked = Some(idx);
                        }
                    }
                    prune_textures(&mut app.textures);
                    if let Some(idx) = clicked {
                        app.selected = idx;
                        app.restore_selected(ctx);
                    }
                });
        });
}

fn draw_row(
    ui: &mut Ui,
    ctx: &egui::Context,
    textures: &mut HashMap<i64, TextureHandle>,
    row: &ListRow,
    selected: bool,
) -> bool {
    let height = if row.kind == ClipKind::Image {
        44.0
    } else {
        28.0
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, ROW_HOVER);
    }
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
            CornerRadius::ZERO,
            MARK,
        );
    }

    let mut x = rect.left() + 10.0;
    let content_right = draw_copied_at(ui, rect, row.created_at);
    if row.kind == ClipKind::Image {
        if response.rect.is_positive() && ui.is_rect_visible(rect) {
            if let Some(tex) = texture_for(ctx, textures, row) {
                let thumb = egui::Rect::from_min_size(
                    Pos2::new(x, rect.center().y - 16.0),
                    Vec2::splat(32.0),
                );
                ui.painter().image(
                    tex.id(),
                    thumb,
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        x += 40.0;
        let clip = Rect::from_min_max(Pos2::new(x, rect.min.y), Pos2::new(content_right, rect.max.y));
        ui.painter().with_clip_rect(clip).text(
            Pos2::new(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            if row.text.is_empty() {
                "image"
            } else {
                row.text.as_str()
            },
            FontId::proportional(13.0),
            MUTE,
        );
    } else {
        let preview = one_line(&row.text);
        let clip = Rect::from_min_max(Pos2::new(x, rect.min.y), Pos2::new(content_right, rect.max.y));
        ui.painter().with_clip_rect(clip).text(
            Pos2::new(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            preview,
            FontId::monospace(13.0),
            if selected { INK } else { Color32::from_rgb(0xD4, 0xCC, 0xC0) },
        );
    }

    response.clicked_by(PointerButton::Primary)
}

fn texture_for(
    ctx: &egui::Context,
    textures: &mut HashMap<i64, TextureHandle>,
    row: &ListRow,
) -> Option<TextureHandle> {
    if let Some(tex) = textures.get(&row.id) {
        return Some(tex.clone());
    }
    let png = row.thumb_png.as_ref()?;
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ctx.load_texture(format!("thumb-{}", row.id), color, TextureOptions::LINEAR);
    textures.insert(row.id, tex.clone());
    Some(tex)
}

fn prune_textures(textures: &mut HashMap<i64, TextureHandle>) {
    if textures.len() <= TEXTURE_CACHE {
        return;
    }
    let overflow = textures.len() - TEXTURE_CACHE;
    let keys: Vec<i64> = textures.keys().copied().take(overflow).collect();
    for key in keys {
        textures.remove(&key);
    }
}

fn draw_settings(app: &mut ClipiApp, ctx: &egui::Context) {
    if app.hotkey_capture {
        match hotkey::poll_capture(ctx) {
            hotkey::Capture::Combo(combo) => {
                app.settings_draft.hotkey = combo.to_string();
                app.stop_hotkey_capture();
            }
            hotkey::Capture::Cancel => app.stop_hotkey_capture(),
            hotkey::Capture::None => {}
        }
    } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.hide_palette(ctx);
        return;
    }
    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(PAPER).inner_margin(Margin::same(16)))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
            ui.label(RichText::new("History").color(MUTE).small());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Max history entries").color(INK));
                ui.add(
                    egui::DragValue::new(&mut app.settings_draft.max_entries)
                        .range(10..=10_000u32)
                        .speed(1),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Max database size (MB)").color(INK));
                ui.add(
                    egui::DragValue::new(&mut app.settings_draft.max_db_mb)
                        .range(8..=512u32)
                        .speed(1),
                );
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Hotkey").color(MUTE).small());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Open palette").color(INK));
                let label = if app.hotkey_capture {
                    "Press a shortcut...".to_string()
                } else {
                    hotkey::display_label(&app.settings_draft.hotkey)
                };
                let response = ui.add_sized(
                    Vec2::new(200.0, 24.0),
                    egui::Button::new(
                        RichText::new(label).color(if app.hotkey_capture { MARK } else { INK }),
                    )
                    .fill(FIELD)
                    .stroke(Stroke::new(
                        1.0,
                        if app.hotkey_capture { MARK } else { RULE },
                    )),
                );
                if response.clicked() {
                    if app.hotkey_capture {
                        app.stop_hotkey_capture();
                    } else {
                        app.start_hotkey_capture();
                    }
                }
            });
            ui.label(
                RichText::new("Click the field, then press the shortcut. Esc cancels.")
                    .color(MUTE)
                    .small(),
            );
            ui.add_space(6.0);
            ui.label(RichText::new("Database").color(MUTE).small());
            ui.horizontal(|ui| {
                let mut path = app.settings_draft.db_path.display().to_string();
                let edit = ui.add(
                    TextEdit::singleline(&mut path)
                        .desired_width(360.0)
                        .hint_text("path to clipi.db"),
                );
                if edit.changed() {
                    app.settings_draft.db_path = PathBuf::from(path);
                }
                if ui.button("Browse").clicked() {
                    if let Some(picked) = rfd::FileDialog::new()
                        .add_filter("SQLite", &["db", "sqlite"])
                        .set_file_name("clipi.db")
                        .save_file()
                    {
                        app.settings_draft.db_path = picked;
                    }
                }
            });
            ui.label(
                RichText::new("Existing clips stay in the previous file.")
                    .color(MUTE)
                    .small(),
            );
            if let Some(err) = &app.settings_error {
                ui.label(RichText::new(err).color(MARK));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    app.save_settings(ctx);
                }
                if ui.button("Cancel").clicked() {
                    app.hide_palette(ctx);
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("clipi {}", env!("CARGO_PKG_VERSION")))
                    .color(MUTE)
                    .small(),
            );
        });
}

fn fts_query(raw: &str) -> Option<String> {
    let mut tokens = Vec::new();
    for part in raw.split_whitespace() {
        let cleaned: String = part
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        tokens.push(format!("{cleaned}*"));
    }
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

fn rank_fuzzy(mut rows: Vec<ListRow>, query: &str) -> Vec<ListRow> {
    let matcher = SkimMatcherV2::default();
    rows.sort_by(|a, b| {
        let sa = matcher.fuzzy_match(&a.text, query).unwrap_or(0);
        let sb = matcher.fuzzy_match(&b.text, query).unwrap_or(0);
        sb.cmp(&sa)
    });
    rows
}

fn one_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    const MAX: usize = 96;
    if line.chars().count() > MAX {
        let trimmed: String = line.chars().take(MAX).collect();
        format!("{trimmed}...")
    } else {
        line.to_string()
    }
}

fn draw_copied_at(ui: &Ui, rect: Rect, created_at: i64) -> f32 {
    let when = format_copied_at(created_at);
    if when.is_empty() {
        return rect.right() - 8.0;
    }
    let font = FontId::proportional(11.0);
    let galley = ui.fonts(|f| f.layout_no_wrap(when, font, MUTE));
    let width = galley.size().x;
    let pos = Pos2::new(
        rect.right() - 10.0 - width,
        rect.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(pos, galley, MUTE);
    pos.x - 8.0
}

fn format_copied_at(ts: i64) -> String {
    let utc = if ts.abs() > 1_000_000_000_000_000 {
        Some(chrono::DateTime::from_timestamp_nanos(ts))
    } else {
        chrono::DateTime::from_timestamp_micros(ts)
    };
    match utc {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        None => String::new(),
    }
}

fn draw_move_handle(ui: &mut Ui, ctx: &egui::Context) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 14.0),
        Sense::click_and_drag(),
    );
    let pill = Rect::from_center_size(rect.center(), Vec2::new(36.0, 3.0));
    ui.painter()
        .rect_filled(pill, CornerRadius::same(2), MUTE);
    if response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    if response.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    response.drag_stopped()
}

fn place_palette(ctx: &egui::Context, size: Vec2, saved: Option<Pos2>) {
    let pos = saved.unwrap_or_else(|| default_window_pos(ctx, size));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
}

fn center_on_screen(ctx: &egui::Context, size: Vec2) {
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(default_window_pos(ctx, size)));
}

fn default_window_pos(ctx: &egui::Context, size: Vec2) -> Pos2 {
    let monitor = ctx
        .input(|i| i.viewport().monitor_size)
        .unwrap_or(Vec2::new(1280.0, 800.0));
    Pos2::new(
        ((monitor.x - size.x) * 0.5).max(0.0),
        ((monitor.y - size.y) * 0.32).max(0.0),
    )
}

fn wake_window(ctx: &egui::Context, win: &win::Handle) {
    win.show();
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PAPER;
    visuals.window_fill = PAPER;
    visuals.extreme_bg_color = FIELD;
    visuals.override_text_color = Some(INK);
    visuals.selection.bg_fill = MARK;
    visuals.widgets.inactive.bg_fill = FIELD;
    visuals.widgets.hovered.bg_fill = ROW_HOVER;
    visuals.widgets.active.bg_fill = ROW_HOVER;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, RULE);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, MARK);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, MARK);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTE);
    ctx.set_visuals(visuals);
}
