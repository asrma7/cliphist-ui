use crate::{
    config::{self, AppConfig, ClickToCopy, WindowConfig, WindowPosition},
    model::ClipboardItem,
    model::ClipboardKind,
};
use gtk::{gdk, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{cell::RefCell, collections::HashMap, fs, path::PathBuf, rc::Rc};
use tracing::warn;

#[derive(Clone)]
pub struct Ui {
    pub window: gtk::ApplicationWindow,
    pub root: gtk::Box,
    pub search: gtk::SearchEntry,
    pub list: gtk::ListBox,
    pub mode: gtk::Label,
    pub hints: gtk::FlowBox,
    pub status: gtk::Label,
    css: Rc<RefCell<CssProviders>>,
}

impl Ui {
    pub fn new(application: &gtk::Application, config: &AppConfig) -> Self {
        let css = install_css();

        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("Clipboard")
            .default_width(config.window.width)
            .default_height(config.window.height)
            .resizable(false)
            .decorated(false)
            .build();
        window.add_css_class("cliphist-ui-window");
        configure_layer_shell(&window, &config.window);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("cliphist-root");
        window.set_child(Some(&root));

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("cliphist-header");
        root.append(&header);

        let mode = gtk::Label::new(None);
        mode.add_css_class("cliphist-mode");
        mode.set_xalign(0.5);
        mode.set_tooltip_text(Some("Normal mode"));
        mode.set_visible(config.behavior.show_vim_mode);
        set_mode_label(&mode, false);
        header.append(&mode);

        let search = gtk::SearchEntry::new();
        search.add_css_class("cliphist-search");
        search.set_hexpand(true);
        search.set_placeholder_text(Some(&config.search.placeholder));
        header.append(&search);

        let scroller = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        root.append(&scroller);

        let list = gtk::ListBox::new();
        list.add_css_class("cliphist-list");
        list.set_focusable(true);
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(matches!(
            config.behavior.click_to_copy,
            ClickToCopy::Single
        ));
        scroller.set_child(Some(&list));

        let footer = gtk::CenterBox::new();
        footer.add_css_class("cliphist-footer");
        root.append(&footer);

        let hints = gtk::FlowBox::new();
        hints.add_css_class("cliphist-hints");
        hints.set_halign(gtk::Align::Center);
        hints.set_valign(gtk::Align::Center);
        hints.set_selection_mode(gtk::SelectionMode::None);
        hints.set_activate_on_single_click(false);
        hints.set_min_children_per_line(1);
        hints.set_max_children_per_line(12);
        hints.set_column_spacing(6);
        hints.set_row_spacing(4);
        footer.set_center_widget(Some(&hints));

        let status = gtk::Label::new(None);
        status.add_css_class("cliphist-status");
        status.set_hexpand(true);
        status.set_xalign(0.0);
        status.set_wrap(false);
        status.set_visible(false);
        footer.set_start_widget(Some(&status));

        Self {
            window,
            root,
            search,
            list,
            mode,
            hints,
            status,
            css,
        }
    }

    pub fn apply_config(&self, config: &AppConfig) {
        self.window
            .set_default_size(config.window.width, config.window.height);
        apply_layer_shell_config(&self.window, &config.window);
        self.search
            .set_placeholder_text(Some(&config.search.placeholder));
        self.mode.set_visible(config.behavior.show_vim_mode);
        self.list.set_activate_on_single_click(matches!(
            config.behavior.click_to_copy,
            ClickToCopy::Single
        ));
        self.window.queue_resize();
    }

    pub fn reload_css(&self) {
        self.css.borrow_mut().reload();
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    pub fn close(&self) {
        self.window.close();
    }

    pub fn focus_search(&self) {
        self.search.grab_focus();
        self.set_insert_mode(true);
    }

    pub fn focus_list(&self) {
        self.list.grab_focus();
        self.set_insert_mode(false);
    }

    pub fn search_text(&self) -> String {
        self.search.text().to_string()
    }

    pub fn set_status(&self, message: &str) {
        self.hints.set_visible(false);
        self.status.set_visible(true);
        self.status.set_text(message);
    }

    pub fn set_key_hints(&self, hints: &[(&str, &str)]) {
        while let Some(child) = self.hints.first_child() {
            self.hints.remove(&child);
        }

        if hints.is_empty() {
            self.status.set_visible(false);
            self.hints.set_visible(false);
            return;
        }

        for (key, action) in hints {
            self.hints.append(&key_hint_chip(key, action));
        }

        self.status.set_visible(false);
        self.hints.set_visible(true);
    }

    pub fn set_insert_mode(&self, insert: bool) {
        set_mode_label(&self.mode, insert);
    }

    pub fn render(
        &self,
        items: &[&ClipboardItem],
        selected: usize,
        config: &AppConfig,
        thumbnails: &HashMap<String, PathBuf>,
    ) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        for item in items {
            self.list.append(&row_for_item(item, config, thumbnails));
        }

        self.select_index(selected, false);
    }

    pub fn select_index(&self, index: usize, focus_row: bool) {
        let Some(row) = self.list.row_at_index(index as i32) else {
            return;
        };

        self.list.select_row(Some(&row));
        if focus_row {
            row.grab_focus();
        }
    }
}

fn key_hint_chip(key: &str, action: &str) -> gtk::Box {
    let chip = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    chip.add_css_class("cliphist-keyhint");
    chip.set_valign(gtk::Align::Center);

    let key = gtk::Label::new(Some(key));
    key.add_css_class("cliphist-keyhint-key");
    key.set_xalign(0.5);
    chip.append(&key);

    let action = gtk::Label::new(Some(action));
    action.add_css_class("cliphist-keyhint-action");
    action.set_xalign(0.0);
    chip.append(&action);

    chip
}

fn set_mode_label(label: &gtk::Label, insert: bool) {
    label.remove_css_class("cliphist-mode-normal");
    label.remove_css_class("cliphist-mode-insert");

    if insert {
        label.set_text("");
        label.set_tooltip_text(Some("Insert mode"));
        label.add_css_class("cliphist-mode-insert");
    } else {
        label.set_text("");
        label.set_tooltip_text(Some("Normal mode"));
        label.add_css_class("cliphist-mode-normal");
    }
}

fn configure_layer_shell(window: &gtk::ApplicationWindow, config: &WindowConfig) {
    window.init_layer_shell();
    window.set_namespace(Some("cliphist-ui"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_exclusive_zone(0);
    window.set_respect_close(true);

    apply_layer_shell_config(window, config);
}

fn apply_layer_shell_config(window: &gtk::ApplicationWindow, config: &WindowConfig) {
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, false);
        window.set_margin(edge, 0);
    }

    match config.position {
        WindowPosition::Center => {}
        WindowPosition::Offset => {
            anchor(window, Edge::Top, config.offset_y);
            anchor(window, Edge::Left, config.offset_x);
        }
        WindowPosition::Top => {
            anchor(window, Edge::Top, 0);
        }
        WindowPosition::TopLeft => {
            anchor(window, Edge::Top, 0);
            anchor(window, Edge::Left, 0);
        }
        WindowPosition::TopRight => {
            anchor(window, Edge::Top, 0);
            anchor(window, Edge::Right, 0);
        }
        WindowPosition::Bottom => {
            anchor(window, Edge::Bottom, 0);
        }
        WindowPosition::BottomLeft => {
            anchor(window, Edge::Bottom, 0);
            anchor(window, Edge::Left, 0);
        }
        WindowPosition::BottomRight => {
            anchor(window, Edge::Bottom, 0);
            anchor(window, Edge::Right, 0);
        }
    }
}

fn anchor(window: &gtk::ApplicationWindow, edge: Edge, margin: i32) {
    window.set_anchor(edge, true);
    window.set_margin(edge, margin.max(0));
}

#[derive(Default)]
struct CssProviders {
    display: Option<gdk::Display>,
    builtin: Option<gtk::CssProvider>,
    user: Option<gtk::CssProvider>,
}

impl CssProviders {
    fn reload(&mut self) {
        self.remove();

        let Some(display) = gdk::Display::default() else {
            return;
        };

        let builtin = gtk::CssProvider::new();
        builtin.load_from_data(config::css());
        gtk::style_context_add_provider_for_display(
            &display,
            &builtin,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let user = install_user_css(&display);
        self.display = Some(display);
        self.builtin = Some(builtin);
        self.user = user;
    }

    fn remove(&mut self) {
        let display = self.display.clone();

        if let Some(display) = display.as_ref() {
            if let Some(provider) = self.user.take() {
                gtk::style_context_remove_provider_for_display(display, &provider);
            }

            if let Some(provider) = self.builtin.take() {
                gtk::style_context_remove_provider_for_display(display, &provider);
            }
        } else {
            self.user = None;
            self.builtin = None;
        }

        self.display = None;
    }
}

fn install_css() -> Rc<RefCell<CssProviders>> {
    let providers = Rc::new(RefCell::new(CssProviders::default()));
    providers.borrow_mut().reload();
    providers
}

fn install_user_css(display: &gdk::Display) -> Option<gtk::CssProvider> {
    let path = config::style_path()?;

    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => {
            let provider = gtk::CssProvider::new();
            provider.load_from_path(&path);
            gtk::style_context_add_provider_for_display(
                display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            Some(provider)
        }
        Ok(_) => {
            warn!(path = %path.display(), "style.css is not a file; using built-in CSS");
            None
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            warn!(path = %path.display(), error = %err, "failed to read style.css; using built-in CSS");
            None
        }
    }
}

fn row_for_item(
    item: &ClipboardItem,
    config: &AppConfig,
    thumbnails: &HashMap<String, PathBuf>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    row.set_activatable(true);

    let container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    container.set_valign(gtk::Align::Center);

    let kind = gtk::Label::new(Some(item.kind.label()));
    kind.add_css_class("cliphist-kind");
    kind.set_xalign(0.0);
    kind.set_width_chars(7);
    container.append(&kind);

    match item.kind {
        ClipboardKind::Image => {
            let preview = gtk::Box::new(gtk::Orientation::Vertical, 6);
            preview.set_hexpand(true);

            if config.image.show_details {
                let label = gtk::Label::new(Some(&item.visible_preview));
                label.add_css_class("cliphist-preview");
                label.set_xalign(0.0);
                preview.append(&label);
            }

            let thumbnail = image_widget(item, config, thumbnails);
            preview.append(&thumbnail);
            container.append(&preview);
        }
        ClipboardKind::Text | ClipboardKind::Binary => {
            let label = gtk::Label::new(Some(&item.visible_preview));
            label.add_css_class("cliphist-preview");
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_wrap(false);
            label.set_single_line_mode(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            container.append(&label);
        }
    }

    row.set_child(Some(&container));
    row
}

fn image_widget(
    item: &ClipboardItem,
    config: &AppConfig,
    thumbnails: &HashMap<String, PathBuf>,
) -> gtk::Widget {
    let width = config.image.width as i32;
    let height = config.image.height as i32;

    if let Some(path) = thumbnails.get(&item.id) {
        let image = gtk::Image::from_file(path);
        image.set_size_request(width, height);
        image.set_pixel_size(height.max(1));
        image.upcast()
    } else {
        let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        placeholder.add_css_class("cliphist-thumb-placeholder");
        placeholder.set_size_request(width, height);
        placeholder.set_valign(gtk::Align::Center);
        placeholder.set_halign(gtk::Align::Start);

        let label = gtk::Label::new(Some("Loading preview"));
        label.add_css_class("cliphist-muted");
        label.set_halign(gtk::Align::Center);
        label.set_valign(gtk::Align::Center);
        placeholder.append(&label);
        placeholder.upcast()
    }
}
