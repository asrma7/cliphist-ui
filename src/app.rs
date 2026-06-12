use crate::{
    actions, config, model, model::ClipboardItem, model::ClipboardKind, thumbnails::Thumbnailer,
    ui::Ui,
};
use async_channel::Sender;
use gtk::{gdk, gio, glib, prelude::*};
use signal_hook::consts::SIGUSR1;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tracing::{info, warn};

#[derive(Clone, Default)]
pub struct AppRuntime {
    state: Rc<RefCell<Option<Rc<RefCell<AppState>>>>>,
    service_hold: Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
    reload_config_requested: Arc<AtomicBool>,
    signal_poll_installed: Rc<Cell<bool>>,
}

impl AppRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn install_signal_handlers(&self) {
        if self.signal_poll_installed.get() {
            return;
        }

        if let Err(err) =
            signal_hook::flag::register(SIGUSR1, Arc::clone(&self.reload_config_requested))
        {
            warn!(error = %err, "failed to install SIGUSR1 config reload handler");
            return;
        }

        self.signal_poll_installed.set(true);
        let runtime = self.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if runtime
                .reload_config_requested
                .swap(false, Ordering::AcqRel)
            {
                runtime.reload_config_from_signal();
            }

            glib::ControlFlow::Continue
        });
    }

    pub fn handle_command_line(
        &self,
        application: &gtk::Application,
        args: &[OsString],
    ) -> glib::ExitCode {
        let request = match LaunchRequest::parse(args) {
            Ok(request) => request,
            Err(err) => {
                eprintln!("{err}");
                eprintln!("Usage: cliphist-ui [--service]");
                return glib::ExitCode::new(2);
            }
        };

        let state = self.ensure_state(application);
        if request.service {
            self.enable_service(application);
            state.borrow_mut().handle_service_invocation();
        } else {
            state.borrow_mut().show_or_toggle_from_activation();
        }

        glib::ExitCode::SUCCESS
    }

    fn enable_service(&self, application: &gtk::Application) {
        if self.service_hold.borrow().is_some() {
            return;
        }

        *self.service_hold.borrow_mut() = Some(application.hold());
    }

    fn reload_config_from_signal(&self) {
        let Some(state) = self.state.borrow().as_ref().cloned() else {
            return;
        };

        let mut state = state.borrow_mut();
        if state.service_mode {
            state.reload_config();
        } else {
            warn!("received SIGUSR1 outside service mode; ignoring config reload");
        }
    }

    fn ensure_state(&self, application: &gtk::Application) -> Rc<RefCell<AppState>> {
        if let Some(state) = self.state.borrow().as_ref() {
            return Rc::clone(state);
        }

        let config = config::load();
        let (sender, receiver) = async_channel::unbounded::<AppEvent>();
        let thumbnail_cache_dir = config::cache_dir();
        let thumbnailer = Thumbnailer::new(
            config.image.clone(),
            thumbnail_cache_dir.clone(),
            sender.clone(),
            0,
        );
        let ui = Ui::new(application, &config);
        let suppress_selection = Rc::new(Cell::new(false));

        let state = Rc::new(RefCell::new(AppState::new(
            config,
            ui,
            sender,
            thumbnailer,
            thumbnail_cache_dir,
            suppress_selection.clone(),
        )));

        connect_signals(&state, suppress_selection);
        drain_events(&state, receiver);
        *self.state.borrow_mut() = Some(Rc::clone(&state));

        state
    }
}

struct LaunchRequest {
    service: bool,
}

impl LaunchRequest {
    fn parse(args: &[OsString]) -> Result<Self, String> {
        let mut service = false;

        for arg in args.iter().skip(1) {
            if arg == OsStr::new("--service") {
                service = true;
            } else if arg == OsStr::new("--help") || arg == OsStr::new("-h") {
                return Err("Usage: cliphist-ui [--service]".into());
            } else {
                return Err(format!("unknown argument: {}", arg.to_string_lossy()));
            }
        }

        Ok(Self { service })
    }
}

pub enum AppEvent {
    HistoryLoaded {
        generation: u64,
        result: Result<Vec<ClipboardItem>, String>,
    },
    Copied {
        result: Result<(), String>,
    },
    Deleted {
        id: String,
        result: Result<(), String>,
    },
    Cleared {
        result: Result<(), String>,
    },
    RefreshAfterCopy,
    RestoreStatus {
        generation: u64,
    },
    ThumbnailReady {
        generation: u64,
        id: String,
        path: PathBuf,
    },
    ThumbnailFailed {
        generation: u64,
        id: String,
        error: String,
    },
}

struct AppState {
    config: config::AppConfig,
    ui: Ui,
    sender: Sender<AppEvent>,
    thumbnailer: Thumbnailer,
    thumbnail_cache_dir: PathBuf,
    items: Vec<ClipboardItem>,
    visible: Vec<usize>,
    selected: usize,
    reload_generation: u64,
    thumbnail_generation: u64,
    thumbnails: HashMap<String, PathBuf>,
    requested_thumbnails: HashSet<String>,
    failed_thumbnails: HashSet<String>,
    pending_clear: bool,
    insert_mode: bool,
    show_keybind_help: bool,
    loading: bool,
    reload_updates_status: bool,
    status_generation: Cell<u64>,
    copy_holds: Vec<gio::ApplicationHoldGuard>,
    close_after_copy_finishes: bool,
    service_mode: bool,
    suppress_selection: Rc<Cell<bool>>,
}

impl AppState {
    fn new(
        config: config::AppConfig,
        ui: Ui,
        sender: Sender<AppEvent>,
        thumbnailer: Thumbnailer,
        thumbnail_cache_dir: PathBuf,
        suppress_selection: Rc<Cell<bool>>,
    ) -> Self {
        let insert_mode = config.behavior.start_in_insert;
        ui.set_status("No clipboard history");
        ui.set_insert_mode(insert_mode);

        Self {
            config,
            ui,
            sender,
            thumbnailer,
            thumbnail_cache_dir,
            items: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            reload_generation: 0,
            thumbnail_generation: 0,
            thumbnails: HashMap::new(),
            requested_thumbnails: HashSet::new(),
            failed_thumbnails: HashSet::new(),
            pending_clear: false,
            insert_mode,
            show_keybind_help: false,
            loading: false,
            reload_updates_status: false,
            status_generation: Cell::new(0),
            copy_holds: Vec::new(),
            close_after_copy_finishes: false,
            service_mode: false,
            suppress_selection,
        }
    }

    fn set_service_mode(&mut self, service_mode: bool) {
        self.service_mode = service_mode;
    }

    fn handle_service_invocation(&mut self) {
        self.set_service_mode(true);
        if self.ui.is_visible() {
            self.dismiss();
        } else {
            self.prepare_background();
        }
    }

    fn prepare_background(&mut self) {
        if self.config.behavior.reload_on_open && !self.loading && self.items.is_empty() {
            self.reload();
        }
    }

    fn show_or_toggle_from_activation(&mut self) {
        if self.service_mode && self.ui.is_visible() {
            self.dismiss();
        } else {
            self.show_from_activation();
        }
    }

    fn show_from_activation(&mut self) {
        if self.config.behavior.reload_on_open && (!self.loading || !self.reload_updates_status) {
            self.reload();
        } else if !self.config.behavior.reload_on_open {
            self.refresh_filter(true, true);
        }

        self.ui.present();
        if self.config.behavior.start_in_insert {
            self.enter_insert_mode();
        } else {
            self.enter_normal_mode();
        }
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::HistoryLoaded { generation, result } => {
                if generation != self.reload_generation {
                    return;
                }

                self.loading = false;
                match result {
                    Ok(items) => {
                        let update_status = self.reload_updates_status;
                        self.items = items;
                        self.selected = 0;
                        self.pending_clear = false;
                        self.show_keybind_help = false;
                        self.refresh_filter(true, update_status);
                    }
                    Err(err) if self.reload_updates_status => {
                        self.set_status(&format!("Failed to load history: {err}"));
                    }
                    Err(_) => {}
                }
            }
            AppEvent::Copied { result } => {
                let should_close = self.close_after_copy_finishes;
                self.close_after_copy_finishes = false;

                match result {
                    Ok(()) if self.config.behavior.close_on_copy => {
                        if self.service_mode {
                            self.schedule_refresh_after_copy();
                        }
                    }
                    Ok(()) => self.schedule_refresh_after_copy(),
                    Err(err) => self.set_status(&format!("Copy failed: {err}")),
                }
                self.release_copy_hold();

                if should_close {
                    self.ui.close();
                }
            }
            AppEvent::Deleted { id, result } => match result {
                Ok(()) => self.set_status("Deleted selected entry"),
                Err(err) => self.set_status(&format!("Delete failed for {id}: {err}")),
            },
            AppEvent::Cleared { result } => match result {
                Ok(()) => self.set_status("History cleared"),
                Err(err) => self.set_status(&format!("Clear failed: {err}")),
            },
            AppEvent::RefreshAfterCopy => self.reload_silent(),
            AppEvent::RestoreStatus { generation } => {
                if self.status_generation.get() == generation {
                    self.set_default_status();
                }
            }
            AppEvent::ThumbnailReady {
                generation,
                id,
                path,
            } => {
                if generation != self.thumbnail_generation {
                    return;
                }

                self.requested_thumbnails.remove(&id);
                self.failed_thumbnails.remove(&id);
                self.thumbnails.insert(id.clone(), path);
                if self.is_visible_id(&id) {
                    self.render_visible();
                }
            }
            AppEvent::ThumbnailFailed {
                generation,
                id,
                error,
            } => {
                if generation != self.thumbnail_generation {
                    return;
                }

                self.requested_thumbnails.remove(&id);
                self.failed_thumbnails.insert(id.clone());
                if self.selected_item().is_some_and(|item| item.id == id) {
                    self.set_status(&format!("Image preview failed: {error}"));
                }
            }
        }
    }

    fn handle_normal_key(&mut self, key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
        let character = key.to_unicode();

        let ctrl = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        if ctrl && matches!(character, Some('r') | Some('R')) {
            self.show_keybind_help = false;
            self.reload();
            return true;
        }

        if key == gdk::Key::Escape {
            if self.pending_clear {
                self.pending_clear = false;
                self.set_default_status();
            } else {
                self.dismiss();
            }
            return true;
        }

        if key == gdk::Key::Down || matches!(character, Some('j')) {
            self.show_keybind_help = false;
            self.move_selection(1);
            return true;
        }

        if key == gdk::Key::Up || matches!(character, Some('k')) {
            self.show_keybind_help = false;
            self.move_selection(-1);
            return true;
        }

        match character {
            Some('?') => {
                self.pending_clear = false;
                self.show_keybind_help = !self.show_keybind_help;
                self.set_default_status();
                true
            }
            Some('/') => {
                self.show_keybind_help = false;
                self.enter_insert_mode();
                true
            }
            Some('g') => {
                self.show_keybind_help = false;
                self.select_first();
                true
            }
            Some('G') => {
                self.show_keybind_help = false;
                self.select_last();
                true
            }
            Some('q') => {
                self.dismiss();
                true
            }
            Some('y') => {
                self.show_keybind_help = false;
                self.copy_selected();
                true
            }
            Some('d') => {
                self.show_keybind_help = false;
                self.delete_selected();
                true
            }
            Some('D') => {
                self.show_keybind_help = false;
                self.confirm_or_clear_all();
                true
            }
            Some('r') => {
                self.show_keybind_help = false;
                self.reload();
                true
            }
            _ if key == gdk::Key::Return || key == gdk::Key::KP_Enter => {
                self.show_keybind_help = false;
                self.copy_selected();
                true
            }
            _ => false,
        }
    }

    fn reload(&mut self) {
        self.reload_with_status(true);
    }

    fn reload_silent(&mut self) {
        self.reload_with_status(false);
    }

    fn reload_config(&mut self) {
        let old_config = self.config.clone();
        let new_config = config::load();
        let new_cache_dir = config::cache_dir();
        let reparse_items = old_config.list != new_config.list;
        let reload_thumbnails =
            old_config.image != new_config.image || self.thumbnail_cache_dir != new_cache_dir;
        let restart_loading = self.loading;

        self.config = new_config;
        self.pending_clear = false;
        self.show_keybind_help = false;

        self.ui.reload_css();
        self.ui.apply_config(&self.config);

        if reparse_items {
            self.reparse_items();
        }

        if reload_thumbnails {
            self.reload_thumbnailer(new_cache_dir);
        }

        if restart_loading {
            self.reload_silent();
        }

        self.refresh_filter(false, false);
        self.set_temporary_status("Configuration reloaded", Duration::from_millis(1200));
        info!("configuration reloaded from SIGUSR1");
    }

    fn reparse_items(&mut self) {
        self.items = self
            .items
            .iter()
            .filter_map(|item| {
                ClipboardItem::parse(&item.raw_line, self.config.list.max_text_chars)
            })
            .collect();
    }

    fn reload_thumbnailer(&mut self, cache_dir: PathBuf) {
        self.thumbnail_generation = self.thumbnail_generation.wrapping_add(1);
        self.thumbnail_cache_dir = cache_dir;
        self.thumbnailer = Thumbnailer::new(
            self.config.image.clone(),
            self.thumbnail_cache_dir.clone(),
            self.sender.clone(),
            self.thumbnail_generation,
        );
        self.thumbnails.clear();
        self.requested_thumbnails.clear();
        self.failed_thumbnails.clear();
    }

    fn reload_with_status(&mut self, update_status: bool) {
        self.pending_clear = false;
        self.show_keybind_help = false;
        self.loading = true;
        self.reload_updates_status = update_status;
        self.reload_generation = self.reload_generation.wrapping_add(1);
        if update_status {
            self.set_status("Loading clipboard history...");
        }
        actions::reload(
            self.sender.clone(),
            self.reload_generation,
            self.config.list.max_text_chars,
            self.config.image.clone(),
            self.thumbnail_cache_dir.clone(),
        );
    }

    fn dismiss(&mut self) {
        if self.service_mode {
            self.ui.hide();
        } else {
            self.ui.close();
        }
    }

    fn refresh_filter(&mut self, reset_selection: bool, update_status: bool) {
        let query = model::normalize_search(&self.ui.search_text());
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if query.is_empty() || item.search_text.contains(&query) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();

        if reset_selection {
            self.selected = 0;
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len().saturating_sub(1);
        }

        self.render_visible();
        if update_status {
            self.set_default_status();
        }
    }

    fn render_visible(&mut self) {
        let rows: Vec<&ClipboardItem> = self
            .visible
            .iter()
            .filter_map(|index| self.items.get(*index))
            .collect();

        self.suppress_selection.set(true);
        self.ui
            .render(&rows, self.selected, &self.config, &self.thumbnails);
        self.suppress_selection.set(false);
        self.request_nearby_thumbnails();
    }

    fn set_default_status(&self) {
        if self.loading && self.reload_updates_status {
            self.set_status("Loading clipboard history...");
        } else if self.items.is_empty() {
            self.set_status("No clipboard history");
        } else if self.visible.is_empty() {
            self.set_status("No matching clipboard entries");
        } else if self.insert_mode {
            self.set_key_hints(&[("type", "search"), ("Esc", "normal")]);
        } else if self.show_keybind_help {
            self.set_key_hints(&[
                ("Enter/y", "copy"),
                ("/", "search"),
                ("g/G", "first/last"),
                ("d", "delete"),
                ("D", "clear"),
                ("r/C-r", "reload"),
                ("Esc/q", "quit"),
                ("?", "hide"),
            ]);
        } else {
            self.set_key_hints(&[
                ("Enter/y", "copy"),
                ("/", "search"),
                ("j/k", "move"),
                ("?", "more"),
                ("q", "quit"),
            ]);
        }
    }

    fn request_nearby_thumbnails(&mut self) {
        if self.visible.is_empty() {
            return;
        }

        let start = self.selected.saturating_sub(8);
        let end = (self.selected + 40).min(self.visible.len());
        for visible_index in start..end {
            let Some(item) = self.items.get(self.visible[visible_index]) else {
                continue;
            };

            if item.kind != ClipboardKind::Image
                || self.thumbnails.contains_key(&item.id)
                || self.requested_thumbnails.contains(&item.id)
                || self.failed_thumbnails.contains(&item.id)
            {
                continue;
            }

            if self.thumbnailer.request(item) {
                self.requested_thumbnails.insert(item.id.clone());
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }

        let last = self.visible.len() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.suppress_selection.set(true);
        self.ui.select_index(self.selected, !self.insert_mode);
        self.suppress_selection.set(false);
        self.pending_clear = false;
        self.request_nearby_thumbnails();
    }

    fn select_first(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = 0;
        self.suppress_selection.set(true);
        self.ui.select_index(self.selected, true);
        self.suppress_selection.set(false);
        self.pending_clear = false;
        self.request_nearby_thumbnails();
    }

    fn select_last(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = self.visible.len() - 1;
        self.suppress_selection.set(true);
        self.ui.select_index(self.selected, true);
        self.suppress_selection.set(false);
        self.pending_clear = false;
        self.request_nearby_thumbnails();
    }

    fn copy_selected(&mut self) {
        self.pending_clear = false;
        self.show_keybind_help = false;
        let Some(item) = self.selected_item() else {
            self.set_status("No clipboard entry selected");
            return;
        };

        let close_after_copy = self.config.behavior.close_on_copy;
        if close_after_copy {
            self.hold_for_copy();
        } else {
            self.set_temporary_status("Copied selected entry", Duration::from_millis(900));
        }

        actions::copy(self.sender.clone(), item, self.config.list.max_text_chars);
        if close_after_copy {
            if self.service_mode {
                self.dismiss();
            } else {
                self.close_after_copy_finishes = true;
                self.ui.hide();
            }
        }
    }

    fn delete_selected(&mut self) {
        self.pending_clear = false;
        self.show_keybind_help = false;
        if self.visible.is_empty() {
            self.set_status("No clipboard entry selected");
            return;
        }

        let item_index = self.visible[self.selected];
        let Some(item) = self.items.get(item_index).cloned() else {
            self.set_status("Selected entry no longer exists");
            self.refresh_filter(false, false);
            return;
        };

        self.items.remove(item_index);
        self.refresh_filter(false, false);
        self.set_status("Deleting selected entry...");
        actions::delete(self.sender.clone(), item.id, item.raw_line);
    }

    fn confirm_or_clear_all(&mut self) {
        self.show_keybind_help = false;
        if !self.pending_clear {
            self.pending_clear = true;
            self.set_status("Clear all clipboard history? Press D again to confirm, Esc to cancel");
            return;
        }

        self.pending_clear = false;
        self.items.clear();
        self.visible.clear();
        self.selected = 0;
        self.render_visible();
        self.set_status("Clearing clipboard history...");
        actions::clear_all(self.sender.clone());
    }

    fn enter_insert_mode(&mut self) {
        self.pending_clear = false;
        self.show_keybind_help = false;
        self.set_insert_mode(true);
        self.ui.focus_search();
    }

    fn exit_insert_mode(&mut self) {
        self.enter_normal_mode();
    }

    fn leave_search_if_pointer_outside(&mut self, x: f64, y: f64) {
        if pointer_inside_search(&self.ui.root, &self.ui.search, x, y) {
            return;
        }

        self.enter_normal_mode();
    }

    fn enter_normal_mode(&mut self) {
        self.pending_clear = false;
        self.show_keybind_help = false;
        self.set_insert_mode(false);
        self.ui.focus_list();
    }

    fn set_insert_mode(&mut self, insert_mode: bool) {
        self.insert_mode = insert_mode;
        self.ui.set_insert_mode(insert_mode);
        self.set_default_status();
    }

    fn selected_item(&self) -> Option<ClipboardItem> {
        self.visible
            .get(self.selected)
            .and_then(|index| self.items.get(*index))
            .cloned()
    }

    fn is_visible_id(&self, id: &str) -> bool {
        self.visible
            .iter()
            .filter_map(|index| self.items.get(*index))
            .any(|item| item.id == id)
    }

    fn set_status(&self, message: &str) {
        self.next_status_generation();
        self.ui.set_status(message);
    }

    fn set_temporary_status(&self, message: &str, duration: Duration) {
        let generation = self.next_status_generation();
        self.ui.set_status(message);

        let sender = self.sender.clone();
        thread::spawn(move || {
            thread::sleep(duration);
            let _ = sender.send_blocking(AppEvent::RestoreStatus { generation });
        });
    }

    fn schedule_refresh_after_copy(&self) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            for delay in [
                Duration::from_millis(250),
                Duration::from_millis(750),
                Duration::from_millis(1500),
            ] {
                thread::sleep(delay);
                let _ = sender.send_blocking(AppEvent::RefreshAfterCopy);
            }
        });
    }

    fn hold_for_copy(&mut self) {
        if let Some(application) = self.ui.window.application() {
            self.copy_holds.push(application.hold());
        }
    }

    fn release_copy_hold(&mut self) {
        self.copy_holds.pop();
    }

    fn set_key_hints(&self, hints: &[(&str, &str)]) {
        self.next_status_generation();
        if self.config.behavior.show_keybinds {
            self.ui.set_key_hints(hints);
        } else {
            self.ui.set_key_hints(&[]);
        }
    }

    fn next_status_generation(&self) -> u64 {
        let generation = self.status_generation.get().wrapping_add(1);
        self.status_generation.set(generation);
        generation
    }
}

fn connect_signals(state: &Rc<RefCell<AppState>>, suppress_selection: Rc<Cell<bool>>) {
    let ui = state.borrow().ui.clone();

    let close_state = Rc::clone(state);
    ui.window.connect_close_request(move |_| {
        let mut state = close_state.borrow_mut();
        if state.service_mode {
            state.dismiss();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    let search_state = Rc::clone(state);
    ui.search.connect_search_changed(move |_| {
        let mut state = search_state.borrow_mut();
        state.pending_clear = false;
        state.show_keybind_help = false;
        state.refresh_filter(true, true);
    });

    let search_focus_state = Rc::clone(state);
    let search_focus_controller = gtk::EventControllerFocus::new();
    search_focus_controller.connect_enter(move |_| {
        if let Ok(mut state) = search_focus_state.try_borrow_mut() {
            state.pending_clear = false;
            state.show_keybind_help = false;
            state.set_insert_mode(true);
        }
    });

    let search_blur_state = Rc::clone(state);
    search_focus_controller.connect_leave(move |_| {
        if let Ok(mut state) = search_blur_state.try_borrow_mut() {
            state.set_insert_mode(false);
        }
    });
    ui.search.add_controller(search_focus_controller);

    let root_click_state = Rc::clone(state);
    let root_click_controller = gtk::GestureClick::new();
    root_click_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    root_click_controller.connect_pressed(move |_, _, x, y| {
        if let Ok(mut state) = root_click_state.try_borrow_mut() {
            state.leave_search_if_pointer_outside(x, y);
        }
    });
    ui.root.add_controller(root_click_controller);

    let selection_state = Rc::clone(state);
    ui.list.connect_row_selected(move |_, row| {
        if suppress_selection.get() {
            return;
        }

        let Some(row) = row else {
            return;
        };

        let mut state = selection_state.borrow_mut();
        state.selected = row.index().max(0) as usize;
        state.pending_clear = false;
        state.request_nearby_thumbnails();
    });

    let hover_list = ui.list.clone();
    let hover_controller = gtk::EventControllerMotion::new();
    hover_controller.connect_motion(move |_, _, y| {
        let Some(row) = hover_list.row_at_y(y as i32) else {
            return;
        };

        hover_list.select_row(Some(&row));
    });
    ui.list.add_controller(hover_controller);

    let activation_state = Rc::clone(state);
    ui.list.connect_row_activated(move |_, row| {
        let mut state = activation_state.borrow_mut();
        state.selected = row.index().max(0) as usize;
        state.copy_selected();
    });

    let search_key_state = Rc::clone(state);
    let search_key_controller = gtk::EventControllerKey::new();
    search_key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    search_key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            search_key_state.borrow_mut().exit_insert_mode();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    ui.search.add_controller(search_key_controller);

    let list_key_state = Rc::clone(state);
    let list_key_controller = gtk::EventControllerKey::new();
    list_key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    list_key_controller.connect_key_pressed(move |_, key, _, modifiers| {
        if list_key_state
            .borrow_mut()
            .handle_normal_key(key, modifiers)
        {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    ui.list.add_controller(list_key_controller);
}

fn drain_events(state: &Rc<RefCell<AppState>>, receiver: async_channel::Receiver<AppEvent>) {
    let state = Rc::clone(state);
    glib::timeout_add_local(Duration::from_millis(16), move || {
        while let Ok(event) = receiver.try_recv() {
            state.borrow_mut().handle_event(event);
        }
        glib::ControlFlow::Continue
    });
}

fn pointer_inside_search(root: &gtk::Box, search: &gtk::SearchEntry, x: f64, y: f64) -> bool {
    search.compute_bounds(root).is_some_and(|bounds| {
        let x = x as f32;
        let y = y as f32;
        x >= bounds.x()
            && x <= bounds.x() + bounds.width()
            && y >= bounds.y()
            && y <= bounds.y() + bounds.height()
    })
}
