//! Logs and interactive terminal dialogs.

use crate::models::{Engine, HostId, LogChunk, LogFilter, TermKind, log_visible};
use crate::widgets;
use crate::{window, worker};
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use vte4::prelude::*;

struct LogsUi {
    chunks: RefCell<Vec<LogChunk>>,
    buffer: gtk::TextBuffer,
    view: gtk::TextView,
    query: gtk::SearchEntry,
    filter: Rc<RefCell<LogFilter>>,
    follow: gtk::Switch,
}

thread_local! {
    static LOGS: RefCell<HashMap<u64, Rc<LogsUi>>> = RefCell::new(HashMap::new());
    static TERMS: RefCell<HashMap<u64, vte4::Terminal>> = RefCell::new(HashMap::new());
    static SEQ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub fn next_seq() -> u64 {
    SEQ.with(|s| {
        let n = s.get() + 1;
        s.set(n);
        n
    })
}

pub fn apply_chunk(chunk: LogChunk) {
    let Some(ui) = LOGS.with(|m| m.borrow().get(&chunk.seq).cloned()) else { return };
    {
        let mut chunks = ui.chunks.borrow_mut();
        chunks.push(chunk);
        if chunks.len() > 8000 {
            let drop = chunks.len() - 8000;
            chunks.drain(..drop);
        }
    }
    rebuild(&ui);
}

pub fn apply_done(_seq: u64, result: Result<(), String>) {
    if let Err(err) = result {
        window::toast(&err);
    }
}

fn rebuild(ui: &LogsUi) {
    let query = ui.query.text().to_string();
    let filter = *ui.filter.borrow();
    let mut text = String::new();
    for chunk in ui.chunks.borrow().iter() {
        if log_visible(chunk, filter, &query) {
            text.push_str(&chunk.text);
            if !chunk.text.ends_with('\n') {
                text.push('\n');
            }
        }
    }
    ui.buffer.set_text(&text);
    if ui.follow.is_active() {
        let mut end = ui.buffer.end_iter();
        ui.view.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
    }
}

pub fn apply_bytes(seq: u64, bytes: Vec<u8>) {
    TERMS.with(|m| {
        if let Some(term) = m.borrow().get(&seq) {
            term.feed(&bytes);
        }
    });
}

pub fn apply_term_done(seq: u64, result: Result<(), String>) {
    if let Err(err) = result {
        window::toast(&err);
        TERMS.with(|m| {
            if let Some(term) = m.borrow().get(&seq) {
                term.feed(format!("\r\n[{err}]\r\n").as_bytes());
            }
        });
    }
}

/// A logs view for a tab: filter, follow, wrap, save. Returns the widget and the
/// stream's sequence number (stop it with [`stop`]).
pub fn logs_view(host: HostId, engine: Engine, names: Vec<String>, label: &str) -> (gtk::Box, u64) {
    let seq = next_seq();
    let root = widgets::vbox(0);
    root.add_css_class("tab-logs");
    let bar = widgets::hbox(6);
    bar.add_css_class("tab-subbar");
    let query = gtk::SearchEntry::new();
    query.set_placeholder_text(Some("Filter logs"));
    query.set_width_chars(28);
    bar.append(&query);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    let wrap = gtk::ToggleButton::new();
    wrap.set_icon_name("format-justify-left-symbolic");
    wrap.add_css_class("flat");
    wrap.set_tooltip_text(Some("Wrap long lines"));
    wrap.set_active(true);
    bar.append(&wrap);
    let follow = gtk::ToggleButton::new();
    follow.set_icon_name("go-bottom-symbolic");
    follow.add_css_class("flat");
    follow.set_tooltip_text(Some("Follow new lines"));
    follow.set_active(true);
    bar.append(&follow);
    let save = widgets::icon_button("document-save-symbolic", "Save the logs to a file");
    bar.append(&save);
    let clear_b = widgets::icon_button("edit-clear-all-symbolic", "Clear what's shown");
    bar.append(&clear_b);
    root.append(&bar);

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let view = gtk::TextView::with_buffer(&buffer);
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_monospace(true);
    view.add_css_class("logs-view");
    view.set_left_margin(14);
    view.set_right_margin(14);
    view.set_top_margin(10);
    view.set_bottom_margin(10);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .child(&view)
        .build();
    scroll.add_css_class("tab-well");
    root.append(&scroll);

    let follow_switch = gtk::Switch::new();
    follow_switch.set_active(true);
    let ui = Rc::new(LogsUi {
        chunks: RefCell::new(Vec::new()),
        buffer: buffer.clone(),
        view: view.clone(),
        query: query.clone(),
        filter: Rc::new(RefCell::new(LogFilter::All)),
        follow: follow_switch.clone(),
    });
    let u = ui.clone();
    query.connect_search_changed(move |_| rebuild(&u));
    let v = view.clone();
    wrap.connect_toggled(move |b| v.set_wrap_mode(if b.is_active() { gtk::WrapMode::WordChar } else { gtk::WrapMode::None }));
    let names2 = names.clone();
    follow.connect_toggled(move |b| {
        follow_switch.set_active(b.is_active());
        if b.is_active() {
            worker::follow_logs(host, engine, names2.clone(), seq, true);
        } else {
            worker::stop_logs(seq);
        }
    });
    let u = ui.clone();
    clear_b.connect_clicked(move |_| {
        u.chunks.borrow_mut().clear();
        rebuild(&u);
    });
    let b = buffer.clone();
    let label = label.to_string();
    save.connect_clicked(move |_| {
        let text = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
        let chooser = gtk::FileDialog::new();
        chooser.set_initial_name(Some(&format!("{label}.log")));
        chooser.save(window::window().as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
            if let Ok(file) = r
                && let Some(path) = file.path()
            {
                match std::fs::write(&path, &text) {
                    Ok(()) => window::toast(&format!("Saved {}", path.display())),
                    Err(e) => window::toast(&format!("Couldn't save: {e}")),
                }
            }
        });
    });
    LOGS.with(|m| m.borrow_mut().insert(seq, ui));
    worker::follow_logs(host, engine, names, seq, true);
    (root, seq)
}

/// Stop a tab's log stream or terminal.
pub fn stop(seq: u64) {
    worker::stop_logs(seq);
    worker::stop_term(seq);
    LOGS.with(|m| m.borrow_mut().remove(&seq));
    TERMS.with(|m| m.borrow_mut().remove(&seq));
}

/// A terminal for a tab. Copy and paste with Ctrl+Shift+C/V, Shift+Insert,
/// middle-click or the right-click menu.
pub fn term_view(host: HostId, engine: Engine, name: &str, kind: TermKind) -> (gtk::Box, vte4::Terminal, u64) {
    let seq = next_seq();
    let root = widgets::vbox(0);
    root.add_css_class("tab-term");
    let term = vte4::Terminal::new();
    term.add_css_class("session-term");
    term.add_css_class("grabs-keys");
    term.set_hexpand(true);
    term.set_vexpand(true);
    term.set_scrollback_lines(10_000);
    term.set_font(Some(&gtk::pango::FontDescription::from_string("JetBrains Mono 11")));
    apply_term_colors(&term);
    term.connect_commit(move |_, text, _| worker::term_input(seq, text.as_bytes().to_vec()));
    let resize = move |t: &vte4::Terminal| worker::term_resize(seq, t.column_count() as u16, t.row_count() as u16);
    let r = resize;
    term.connect_notify_local(Some("allocated-width"), move |t, _| r(t));
    term.connect_notify_local(Some("allocated-height"), move |t, _| resize(t));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let t = term.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        let ctrl_shift = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK);
        match key {
            gtk::gdk::Key::C | gtk::gdk::Key::c if ctrl_shift => {
                t.copy_clipboard_format(vte4::Format::Text);
                glib::Propagation::Stop
            }
            gtk::gdk::Key::V | gtk::gdk::Key::v if ctrl_shift => {
                t.paste_clipboard();
                glib::Propagation::Stop
            }
            gtk::gdk::Key::Insert if mods.contains(gtk::gdk::ModifierType::SHIFT_MASK) => {
                t.paste_primary();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    });
    term.add_controller(keys);
    let t = term.clone();
    nyx_ui::menu::on_right_click(&term, move |w, x, y| {
        let (c, p) = (t.clone(), t.clone());
        nyx_ui::menu::popup_at(
            w,
            x,
            y,
            vec![
                nyx_ui::menu::item("Copy", move || c.copy_clipboard_format(vte4::Format::Text)).key("Ctrl+Shift+C"),
                nyx_ui::menu::item("Paste", move || p.paste_clipboard()).key("Ctrl+Shift+V"),
            ],
        );
    });
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .hexpand(true)
        .child(&term)
        .build();
    scroll.add_css_class("tab-well");
    scroll.add_css_class("term-well");
    root.append(&scroll);
    TERMS.with(|m| m.borrow_mut().insert(seq, term.clone()));
    worker::start_term(host, engine, name.to_string(), seq, kind);
    (root, term, seq)
}

/// Terminal colours from the theme, so the terminal sits on the window's surface.
fn apply_term_colors(term: &vte4::Terminal) {
    let p = crate::theme::palette();
    let parse = |s: &str| gtk::gdk::RGBA::parse(s).ok();
    let (Some(fg), Some(bg)) = (parse(&p.text), parse(&p.shadow).or(parse(&p.bg))) else { return };
    let bg = gtk::gdk::RGBA::new(bg.red(), bg.green(), bg.blue(), 0.0);
    term.set_color_foreground(&fg);
    term.set_color_background(&bg);
    if let Some(c) = parse(&p.accent) {
        term.set_color_cursor(Some(&c));
    }
}
