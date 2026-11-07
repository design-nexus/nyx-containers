//! The Resources pages (Images, Storage, Networks) as grouped cards: a coloured
//! icon tile, a title with tags, a line of detail, chips for what's attached, a
//! facts column and the item's actions. A page repaints only when what it shows
//! changes, so open menus and armed buttons survive the refresh.

use crate::models::Engine;
use crate::{widgets, window};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Item {
    pub group: String,
    pub engine: Engine,
    pub icon: &'static str,
    /// A swatch name: blue, violet, pink, red, coral, amber, green, teal, graphite.
    pub color: &'static str,
    pub title: String,
    /// (text, kind): kind is "", "accent", "warning" or "danger".
    pub tags: Vec<(String, &'static str)>,
    pub sub: String,
    pub chips: Vec<String>,
    pub facts: Vec<(String, String)>,
    /// Used fraction, for pools.
    pub bar: Option<f64>,
    pub actions: Vec<gtk::Widget>,
}

impl Item {
    fn sig(&self) -> String {
        format!("{}|{}|{:?}|{}|{:?}|{:?}|{:?}", self.group, self.title, self.tags, self.sub, self.chips, self.facts, self.bar.map(|b| (b * 1000.0) as i64))
    }
}

pub struct ResPage {
    list: gtk::Box,
    empty: gtk::Box,
    filter: Cell<Option<Engine>>,
    chips: RefCell<Vec<(Option<Engine>, gtk::ToggleButton)>>,
    sig: RefCell<String>,
}

pub fn build(
    page: &crate::widgets::Page,
    empty: (&str, &str, &str),
    primary: Option<(&str, &str, Rc<dyn Fn()>)>,
) -> Rc<ResPage> {
    page.body.add_css_class("cards-page");
    let tools = widgets::hbox(6);
    tools.add_css_class("cards-tools");
    let chips_box = widgets::hbox(6);
    tools.append(&chips_box);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    tools.append(&spacer);
    if let Some((icon, tip, act)) = primary {
        let b = widgets::primary_button(icon, tip);
        b.connect_clicked(move |_| act());
        tools.append(&b);
    }
    page.body.append(&tools);
    let list = widgets::vbox(0);
    list.add_css_class("cards-list");
    page.body.append(&list);
    let empty = widgets::empty_state(empty.0, empty.1, empty.2, None);
    empty.set_visible(false);
    page.body.append(&empty);
    let ui = Rc::new(ResPage { list, empty, filter: Cell::new(None), chips: RefCell::new(Vec::new()), sig: RefCell::new(String::new()) });
    let mut first: Option<gtk::ToggleButton> = None;
    for (engine, label) in [(None, "All"), (Some(Engine::Docker), "Docker"), (Some(Engine::Incus), "Incus")] {
        let b = gtk::ToggleButton::new();
        b.add_css_class("chip");
        let c = widgets::hbox(6);
        c.append(&widgets::label(label, ""));
        c.append(&widgets::label("", "chip-count"));
        b.set_child(Some(&c));
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
            b.set_active(true);
        }
        let w = Rc::downgrade(&ui);
        b.connect_toggled(move |b| {
            if b.is_active()
                && let Some(ui) = w.upgrade()
            {
                ui.filter.set(engine);
                window::repaint();
            }
        });
        chips_box.append(&b);
        ui.chips.borrow_mut().push((engine, b));
    }
    ui
}

impl ResPage {
    pub fn filter(&self) -> Option<Engine> {
        self.filter.get()
    }

    /// Show `items` (already filtered by host and search; the engine chips apply here).
    pub fn paint(&self, items: Vec<Item>) {
        for (engine, b) in self.chips.borrow().iter() {
            let n = items.iter().filter(|i| engine.is_none_or(|e| i.engine == e) && i.bar.is_none()).count();
            if let Some(count) = b.child().and_then(|c| c.last_child()).and_downcast::<gtk::Label>() {
                count.set_text(&n.to_string());
            }
        }
        let filter = self.filter.get();
        let items: Vec<Item> = items.into_iter().filter(|i| filter.is_none_or(|e| i.engine == e)).collect();
        let narrow = window::is_narrow();
        let sig = format!("{narrow}|{filter:?}|{}", items.iter().map(Item::sig).collect::<Vec<_>>().join("\n"));
        if *self.sig.borrow() == sig {
            return;
        }
        *self.sig.borrow_mut() = sig;
        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        let mut group = String::new();
        let mut first = true;
        let mut box_: Option<gtk::Box> = None;
        for item in items {
            if item.group != group {
                group = item.group.clone();
                let head = widgets::hbox(8);
                head.add_css_class("cards-group");
                if first {
                    head.add_css_class("first");
                }
                first = false;
                head.append(&widgets::label(&group.to_uppercase(), "cards-group-title"));
                self.list.append(&head);
                let b = widgets::vbox(0);
                self.list.append(&b);
                box_ = Some(b);
            }
            if let Some(b) = &box_ {
                b.append(&card(item, narrow));
            }
        }
        let any = self.list.first_child().is_some();
        self.list.set_visible(any);
        self.empty.set_visible(!any);
    }
}

fn card(item: Item, narrow: bool) -> gtk::Box {
    let root = widgets::hbox(14);
    root.add_css_class("wl-card");
    root.add_css_class("res-card");
    let tile = gtk::CenterBox::new();
    tile.add_css_class("res-tile");
    tile.add_css_class(&format!("c-{}", item.color));
    tile.set_size_request(36, 36);
    tile.set_valign(gtk::Align::Center);
    let icon = gtk::Image::from_icon_name(item.icon);
    icon.set_pixel_size(18);
    tile.set_center_widget(Some(&icon));
    root.append(&tile);

    let text = widgets::vbox(3);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title_row = widgets::hbox(7);
    let title = widgets::label(&item.title, "wl-name");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_selectable(true);
    title_row.append(&title);
    for (t, kind) in &item.tags {
        title_row.append(&widgets::tag(t, kind));
    }
    text.append(&title_row);
    if !item.sub.is_empty() {
        let sub = widgets::label(&item.sub, "wl-sub");
        sub.set_xalign(0.0);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        sub.set_tooltip_text(Some(&item.sub));
        text.append(&sub);
    }
    if !item.chips.is_empty() {
        let flow = gtk::FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_max_children_per_line(12);
        flow.set_column_spacing(4);
        flow.set_row_spacing(4);
        flow.set_halign(gtk::Align::Start);
        flow.add_css_class("res-chips");
        for c in &item.chips {
            flow.append(&widgets::label(c, "port-chip"));
        }
        text.append(&flow);
    }
    root.append(&text);

    if !item.facts.is_empty() && !narrow {
        let facts = widgets::vbox(3);
        facts.set_valign(gtk::Align::Center);
        facts.set_size_request(200, -1);
        for (k, v) in &item.facts {
            let row = widgets::hbox(8);
            let key = widgets::label(k, "tab-kv-key res-fact");
            key.set_xalign(0.0);
            key.set_size_request(64, -1);
            row.append(&key);
            let val = widgets::label(v, "res-fact");
            val.set_xalign(0.0);
            val.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            val.set_hexpand(true);
            val.set_selectable(true);
            row.append(&val);
            facts.append(&row);
        }
        root.append(&facts);
    }
    if let Some(frac) = item.bar {
        let bar = gtk::LevelBar::new();
        bar.add_css_class("host-bar");
        bar.set_value(frac.clamp(0.0, 1.0));
        bar.set_size_request(if narrow { 72 } else { 140 }, -1);
        bar.set_valign(gtk::Align::Center);
        root.append(&bar);
    }
    let acts = widgets::hbox(2);
    acts.set_valign(gtk::Align::Center);
    acts.set_halign(gtk::Align::End);
    // A fixed column, so the facts beside it line up from card to card.
    if !narrow {
        acts.set_size_request(170, -1);
    }
    for a in item.actions {
        acts.append(&a);
    }
    root.append(&acts);
    root
}

/// A remove button that needs two clicks.
pub fn remove_button(act: impl Fn() + 'static) -> gtk::Widget {
    let b = widgets::two_click("Remove", "Click again", act);
    b.add_css_class("res-remove");
    b.upcast()
}

/// "Nov 2, 2026" from Unix seconds or an RFC 3339 time.
pub fn date(text: &str) -> Option<String> {
    let secs = text.trim().parse::<i64>().ok().or_else(|| crate::models::parse_rfc3339(text))?;
    let d = gtk::glib::DateTime::from_unix_local(secs).ok()?;
    d.format("%b %-d, %Y").ok().map(|s| s.to_string())
}
