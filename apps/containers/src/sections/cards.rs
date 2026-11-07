//! Workload cards: every VM, LXC container, Docker container and Compose stack as a
//! short full-width card with its status, address and ports, live CPU and memory
//! graphs, and the actions you reach for (web UI, display, terminal, logs, power).
//! The Overview shows them all under a host strip; the type pages show one kind.

use crate::history;
use crate::models::{Engine, HostFilter, HostHealth, HostId, Kind, RunState, Snapshot, Stack, Workload};
use crate::sections::actions;
use crate::widgets::{self, Page};
use crate::{prefs, window};
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    All,
    Vms,
    Lxc,
    Docker,
    Stacks,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Vms,
    Lxc,
    Docker,
    Running,
    Stopped,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Pinned,
    Name,
    Cpu,
}

/// One live graph: a label, the reading, and a sparkline.
#[derive(Clone)]
struct Meter {
    root: gtk::Box,
    value: gtk::Label,
    area: gtk::DrawingArea,
    data: Rc<RefCell<(Vec<f64>, Option<f64>)>>,
}

impl Meter {
    fn new(title: &str) -> Meter {
        let root = widgets::vbox(3);
        root.add_css_class("wl-meter");
        let top = widgets::hbox(6);
        top.append(&widgets::label(title, "wl-meter-label"));
        let value = widgets::label("—", "wl-meter-value");
        value.set_hexpand(true);
        value.set_xalign(1.0);
        value.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        top.append(&value);
        root.append(&top);
        let area = gtk::DrawingArea::new();
        area.add_css_class("wl-spark");
        area.set_content_height(22);
        area.set_hexpand(true);
        let data: Rc<RefCell<(Vec<f64>, Option<f64>)>> = Rc::new(RefCell::new((Vec::new(), None)));
        let d = data.clone();
        area.set_draw_func(move |a, cr, w, h| {
            let (vals, cap) = &*d.borrow();
            draw_spark(a, cr, w, h, vals, *cap);
        });
        root.append(&area);
        Meter { root, value, area, data }
    }

    fn set(&self, text: &str, vals: Vec<f64>, cap: Option<f64>) {
        self.value.set_text(text);
        *self.data.borrow_mut() = (vals, cap);
        self.area.queue_draw();
    }
}

fn draw_spark(area: &gtk::DrawingArea, cr: &gtk::cairo::Context, w: i32, h: i32, vals: &[f64], cap: Option<f64>) {
    let (w, h) = (w as f64, h as f64);
    let c = area.color();
    let (r, g, b) = (c.red() as f64, c.green() as f64, c.blue() as f64);
    let y = |v: f64| h - 1.5 - v.clamp(0.0, 1.0) * (h - 3.0);
    if let Some(cap) = cap {
        cr.set_source_rgba(r, g, b, 0.28);
        cr.set_line_width(1.0);
        cr.set_dash(&[2.0, 3.0], 0.0);
        cr.move_to(0.0, y(cap).round() + 0.5);
        cr.line_to(w, y(cap).round() + 0.5);
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
    }
    if vals.len() < 2 {
        cr.set_source_rgba(r, g, b, 0.18);
        cr.set_line_width(1.0);
        cr.move_to(0.0, h - 1.5);
        cr.line_to(w, h - 1.5);
        let _ = cr.stroke();
        return;
    }
    // Fill the width; once the history is full, older readings slide off the left.
    let step = w / (vals.len() as f64 - 1.0).max(1.0);
    let start = 0.0;
    cr.move_to(start, y(vals[0]));
    for (i, v) in vals.iter().enumerate().skip(1) {
        cr.line_to(start + step * i as f64, y(*v));
    }
    let path = cr.copy_path();
    cr.line_to(w, h);
    cr.line_to(start, h);
    cr.close_path();
    cr.set_source_rgba(r, g, b, 0.14);
    let _ = cr.fill();
    if let Ok(path) = path {
        cr.append_path(&path);
    }
    cr.set_source_rgba(r, g, b, 1.0);
    cr.set_line_width(1.5);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    let _ = cr.stroke();
}

pub fn short_bytes(n: u64) -> String {
    const G: f64 = 1024.0 * 1024.0 * 1024.0;
    const M: f64 = 1024.0 * 1024.0;
    let f = n as f64;
    if f >= G {
        let v = f / G;
        if v >= 100.0 { format!("{v:.0} GB") } else { format!("{v:.1} GB") }
    } else if f >= M {
        format!("{:.0} MB", f / M)
    } else {
        format!("{:.0} KB", f / 1024.0)
    }
}

fn mem_text(used: Option<u64>, limit: Option<u64>) -> String {
    match (used, limit) {
        (Some(u), Some(l)) if l > 0 => {
            let lt = short_bytes(l);
            let ut = short_bytes(u);
            // "5.1 / 8.0 GB": one unit when both share it.
            match (ut.rsplit_once(' '), lt.rsplit_once(' ')) {
                (Some((uv, uu)), Some((lv, lu))) if uu == lu => format!("{uv} / {lv} {lu}"),
                _ => format!("{ut} / {lt}"),
            }
        }
        (Some(u), _) => short_bytes(u),
        _ => "—".into(),
    }
}

/// What a card shows: one workload, or a Compose stack and its services.
#[derive(Clone)]
enum Item {
    One(Workload),
    Stack(Stack, Vec<Workload>),
}

impl Item {
    fn key(&self) -> String {
        match self {
            Item::One(w) => w.key(),
            Item::Stack(s, _) => format!("stack|{}|{}", s.host.label(), s.name),
        }
    }

    fn name(&self) -> &str {
        match self {
            Item::One(w) => &w.name,
            Item::Stack(s, _) => &s.name,
        }
    }

    fn running(&self) -> bool {
        match self {
            Item::One(w) => matches!(w.state, RunState::Running | RunState::Paused),
            Item::Stack(s, _) => s.running > 0,
        }
    }

    fn live_keys(&self) -> Vec<String> {
        match self {
            Item::One(w) => vec![w.key()],
            Item::Stack(_, services) => services.iter().map(Workload::key).collect(),
        }
    }
}

struct Card {
    root: gtk::Box,
    tile_icon: gtk::Image,
    dot: gtk::Box,
    name: gtk::Label,
    star: gtk::Image,
    tags: gtk::Box,
    sub: gtk::Label,
    info: gtk::Box,
    l1: gtk::Box,
    l2: gtk::Label,
    cpu: Meter,
    mem: Meter,
    idle: gtk::Label,
    acts: gtk::Box,
    services: gtk::Box,
    item: RefCell<Item>,
    /// What the actions and services were built for; rebuilt only when it changes,
    /// so an open menu survives a refresh.
    sig: RefCell<String>,
    svc_sig: RefCell<String>,
    svc_live: RefCell<Vec<(String, gtk::Label, gtk::Label)>>,
    /// What the card last showed; an unchanged refresh leaves its widgets alone.
    data_sig: RefCell<String>,
}

fn data_sig(item: &Item) -> String {
    let minute = crate::models::now_secs() / 60;
    let narrow = window::is_narrow();
    match item {
        Item::One(w) => format!("{w:?}|{minute}|{narrow}|{}", actions::is_pinned(&w.key())),
        Item::Stack(s, services) => format!("{s:?}|{services:?}|{minute}|{narrow}|{:?}", prefs::get().expanded),
    }
}

struct PageUi {
    scope: Scope,
    strip: Option<HostStrip>,
    chips: gtk::Box,
    chip_buttons: RefCell<Vec<(Filter, gtk::ToggleButton)>>,
    list: gtk::Box,
    empty: gtk::Box,
    filter: Cell<Filter>,
    sort: Cell<Sort>,
    sort_label: gtk::Label,
    cards: RefCell<HashMap<String, Rc<Card>>>,
    layout: RefCell<Vec<String>>,
}

thread_local! {
    static PAGES: RefCell<Vec<Rc<PageUi>>> = const { RefCell::new(Vec::new()) };
    static HEALTH: RefCell<Vec<HostHealth>> = const { RefCell::new(Vec::new()) };
    static LAST: RefCell<Option<(Snapshot, HostFilter, String)>> = const { RefCell::new(None) };
}

pub fn build_overview(page: &Page) {
    build(page, Scope::All);
}
pub fn build_vms(page: &Page) {
    build(page, Scope::Vms);
}
pub fn build_lxc(page: &Page) {
    build(page, Scope::Lxc);
}
pub fn build_docker(page: &Page) {
    build(page, Scope::Docker);
}
pub fn build_stacks(page: &Page) {
    build(page, Scope::Stacks);
}

fn build(page: &Page, scope: Scope) {
    page.body.add_css_class("cards-page");
    let strip = (scope == Scope::All).then(HostStrip::new);
    if let Some(s) = &strip {
        page.body.append(&s.root);
        if window::is_narrow() {
            s.engines.set_visible(false);
            s.guests.root.set_visible(false);
        }
    }
    let tools = widgets::hbox(6);
    tools.add_css_class("cards-tools");
    let chips = widgets::hbox(6);
    tools.append(&chips);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    tools.append(&spacer);
    let sort_button = gtk::Button::new();
    sort_button.add_css_class("cards-sort");
    let sort_box = widgets::hbox(6);
    sort_box.append(&widgets::label("Sort", "dim"));
    let sort_label = widgets::label("Pinned", "");
    sort_box.append(&sort_label);
    sort_box.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
    sort_button.set_child(Some(&sort_box));
    sort_button.set_tooltip_text(Some("How the cards are ordered"));
    tools.append(&sort_button);
    page.body.append(&tools);

    let list = widgets::vbox(0);
    list.add_css_class("cards-list");
    page.body.append(&list);
    let (title, desc) = match scope {
        Scope::All => ("Nothing here yet", "Create a VM, a Linux container or a Docker container to see it here."),
        Scope::Vms => ("No virtual machines", "Incus runs full VMs with their own kernel and display."),
        Scope::Lxc => ("No Linux containers", "Incus system containers boot a whole distro and share this kernel."),
        Scope::Docker => ("No Docker containers", "Run an image, or deploy a Compose stack."),
        Scope::Stacks => ("No Compose stacks", "Stacks show up from Docker Compose labels, or create one from a compose.yaml."),
    };
    let empty = widgets::empty_state(
        "package-x-generic-symbolic",
        title,
        desc,
        Some(("New workload", Box::new(move || crate::sections::create::open(None)))),
    );
    empty.set_visible(false);
    page.body.append(&empty);

    let ui = Rc::new(PageUi {
        scope,
        strip,
        chips,
        chip_buttons: RefCell::new(Vec::new()),
        list,
        empty,
        filter: Cell::new(Filter::All),
        sort: Cell::new(Sort::Pinned),
        sort_label,
        cards: RefCell::new(HashMap::new()),
        layout: RefCell::new(Vec::new()),
    });
    let weak = Rc::downgrade(&ui);
    sort_button.connect_clicked(move |b| {
        let Some(ui) = weak.upgrade() else { return };
        let current = ui.sort.get();
        let mk = |label: &str, sort: Sort| {
            let w = Rc::downgrade(&ui);
            nyx_ui::menu::check(label, current == sort, move |_| {
                if let Some(ui) = w.upgrade() {
                    ui.sort.set(sort);
                    ui.sort_label.set_text(match sort {
                        Sort::Pinned => "Pinned",
                        Sort::Name => "Name",
                        Sort::Cpu => "CPU",
                    });
                    repaint(&ui);
                }
            })
        };
        nyx_ui::menu::popup_below(
            b,
            vec![mk("Pinned first, then name", Sort::Pinned), mk("Name", Sort::Name), mk("CPU use", Sort::Cpu)],
        );
    });
    build_chips(&ui);
    PAGES.with(|p| p.borrow_mut().push(ui.clone()));
    if let Some((snap, host, q)) = LAST.with(|l| l.borrow().clone()) {
        paint(&ui, &snap, host, &q);
    }
}

fn build_chips(ui: &Rc<PageUi>) {
    let filters: &[(Filter, &str)] = match ui.scope {
        Scope::All => &[
            (Filter::All, "All"),
            (Filter::Vms, "VMs"),
            (Filter::Lxc, "LXC"),
            (Filter::Docker, "Docker"),
            (Filter::Running, "Running"),
        ],
        _ => &[(Filter::All, "All"), (Filter::Running, "Running"), (Filter::Stopped, "Stopped")],
    };
    let mut first: Option<gtk::ToggleButton> = None;
    let mut out = Vec::new();
    for (f, label) in filters {
        let b = gtk::ToggleButton::new();
        b.add_css_class("chip");
        let content = widgets::hbox(6);
        content.append(&widgets::label(label, ""));
        content.append(&widgets::label("", "chip-count"));
        b.set_child(Some(&content));
        if let Some(g) = &first {
            b.set_group(Some(g));
        } else {
            first = Some(b.clone());
        }
        b.set_active(*f == Filter::All);
        let (w, f) = (Rc::downgrade(ui), *f);
        b.connect_toggled(move |b| {
            if b.is_active()
                && let Some(ui) = w.upgrade()
            {
                ui.filter.set(f);
                repaint(&ui);
            }
        });
        ui.chips.append(&b);
        out.push((f, b));
    }
    *ui.chip_buttons.borrow_mut() = out;
}

fn repaint(ui: &Rc<PageUi>) {
    if let Some((snap, host, q)) = LAST.with(|l| l.borrow().clone()) {
        paint(ui, &snap, host, &q);
    }
}

pub fn refresh(snapshot: &Snapshot, host: HostFilter, query: &str) {
    LAST.with(|l| *l.borrow_mut() = Some((snapshot.clone(), host, query.to_string())));
    for ui in PAGES.with(|p| p.borrow().clone()) {
        paint(&ui, snapshot, host, query);
    }
}

pub fn apply_health(list: Vec<HostHealth>) {
    HEALTH.with(|h| *h.borrow_mut() = list);
    let host = window::host_filter();
    let snap = LAST.with(|l| l.borrow().as_ref().map(|(s, _, _)| s.clone()));
    for ui in PAGES.with(|p| p.borrow().clone()) {
        if let (Some(strip), Some(snap)) = (&ui.strip, &snap) {
            strip.paint(host, snap);
        }
    }
}

pub fn apply_live() {
    for ui in PAGES.with(|p| p.borrow().clone()) {
        for card in ui.cards.borrow().values() {
            paint_live(card);
        }
    }
}

pub fn set_narrow(narrow: bool) {
    for ui in PAGES.with(|p| p.borrow().clone()) {
        if let Some(strip) = &ui.strip {
            strip.engines.set_visible(!narrow);
            strip.guests.root.set_visible(!narrow);
        }
        if narrow {
            ui.list.add_css_class("narrow");
        } else {
            ui.list.remove_css_class("narrow");
        }
        for card in ui.cards.borrow().values() {
            apply_narrow(card, narrow);
        }
    }
}

fn apply_narrow(card: &Card, narrow: bool) {
    card.info.set_visible(!narrow);
    for m in [&card.cpu, &card.mem] {
        m.root.set_size_request(if narrow { 92 } else { 140 }, -1);
    }
    let mut child = card.acts.first_child();
    while let Some(c) = child {
        if c.has_css_class("opt-wide") {
            c.set_visible(!narrow);
        }
        child = c.next_sibling();
    }
    card.acts.set_size_request(if narrow { -1 } else { 196 }, -1);
}

fn matches_query(w: &Workload, q: &str) -> bool {
    q.is_empty()
        || w.name.to_ascii_lowercase().contains(q)
        || w.image.as_deref().is_some_and(|i| i.to_ascii_lowercase().contains(q))
        || w.os.as_deref().is_some_and(|i| i.to_ascii_lowercase().contains(q))
        || w.ip.as_deref().is_some_and(|i| i.contains(q))
        || w.project.as_deref().is_some_and(|p| p.to_ascii_lowercase().contains(q))
}

fn items(ui: &PageUi, snapshot: &Snapshot, host: HostFilter, query: &str) -> Vec<(&'static str, Vec<Item>)> {
    let q = query.trim().to_ascii_lowercase();
    let filter = ui.filter.get();
    let keep = |w: &Workload| -> bool {
        matches_query(w, &q)
            && match filter {
                Filter::All => true,
                Filter::Vms => w.kind == Kind::VirtualMachine,
                Filter::Lxc => w.is_lxc(),
                Filter::Docker => w.engine == Engine::Docker,
                Filter::Running => matches!(w.state, RunState::Running | RunState::Paused),
                Filter::Stopped => !matches!(w.state, RunState::Running | RunState::Paused),
            }
    };
    let all: Vec<&Workload> = snapshot.filtered(host, None).collect();
    let vms: Vec<Item> =
        all.iter().filter(|w| w.kind == Kind::VirtualMachine && keep(w)).map(|w| Item::One((*w).clone())).collect();
    let lxc: Vec<Item> = all.iter().filter(|w| w.is_lxc() && keep(w)).map(|w| Item::One((*w).clone())).collect();
    let mut docker: Vec<Item> = all
        .iter()
        .filter(|w| w.engine == Engine::Docker && w.project.is_none() && keep(w))
        .map(|w| Item::One((*w).clone()))
        .collect();
    let mut stacks: Vec<Item> = Vec::new();
    for stack in snapshot.stacks(host) {
        let services: Vec<Workload> = all
            .iter()
            .filter(|w| w.engine == Engine::Docker && w.host == stack.host && w.project.as_deref() == Some(stack.name.as_str()))
            .map(|w| (*w).clone())
            .collect();
        let name_hit = q.is_empty() || stack.name.to_ascii_lowercase().contains(&q) || services.iter().any(|w| matches_query(w, &q));
        let state_ok = match filter {
            Filter::All | Filter::Docker => true,
            Filter::Running => stack.running > 0,
            Filter::Stopped => stack.running == 0,
            Filter::Vms | Filter::Lxc => false,
        };
        if name_hit && state_ok {
            stacks.push(Item::Stack(stack, services));
        }
    }
    let sort = ui.sort.get();
    let order = |v: &mut Vec<Item>| {
        v.sort_by(|a, b| {
            let (pa, pb) = (actions::is_pinned(&a.key()), actions::is_pinned(&b.key()));
            let name = a.name().to_ascii_lowercase().cmp(&b.name().to_ascii_lowercase());
            match sort {
                Sort::Name => name,
                Sort::Cpu => {
                    let cpu = |i: &Item| history::total(&i.live_keys()).0.unwrap_or(-1.0);
                    cpu(b).partial_cmp(&cpu(a)).unwrap_or(std::cmp::Ordering::Equal).then(name)
                }
                Sort::Pinned => pb.cmp(&pa).then(b.running().cmp(&a.running())).then(name),
            }
        });
    };
    let mut vms = vms;
    let mut lxc = lxc;
    order(&mut vms);
    order(&mut lxc);
    order(&mut docker);
    order(&mut stacks);
    match ui.scope {
        Scope::All => {
            let mut d = docker;
            d.extend(stacks);
            vec![("Virtual machines", vms), ("Linux containers", lxc), ("Docker", d)]
        }
        Scope::Vms => vec![("", vms)],
        Scope::Lxc => vec![("", lxc)],
        Scope::Docker => {
            let mut d = docker;
            d.extend(stacks);
            vec![("", d)]
        }
        Scope::Stacks => vec![("", stacks)],
    }
}

fn paint(ui: &Rc<PageUi>, snapshot: &Snapshot, host: HostFilter, query: &str) {
    if let Some(strip) = &ui.strip {
        strip.paint(host, snapshot);
    }
    paint_chip_counts(ui, snapshot, host);
    let groups = items(ui, snapshot, host, query);
    let mut layout = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (title, list) in &groups {
        if list.is_empty() {
            continue;
        }
        if !title.is_empty() {
            layout.push(format!("#{title}#{}", list.len()));
        }
        for item in list {
            let key = item.key();
            seen.insert(key.clone());
            layout.push(key.clone());
            let existing = ui.cards.borrow().get(&key).cloned();
            match existing {
                Some(card) => {
                    let sig = data_sig(item);
                    if *card.data_sig.borrow() != sig {
                        *card.data_sig.borrow_mut() = sig;
                        update_card(&card, item);
                    }
                }
                None => {
                    let card = new_card(item);
                    ui.cards.borrow_mut().insert(key, card);
                }
            }
        }
    }
    ui.cards.borrow_mut().retain(|k, _| seen.contains(k));
    if *ui.layout.borrow() != layout {
        while let Some(c) = ui.list.first_child() {
            ui.list.remove(&c);
        }
        let cards = ui.cards.borrow();
        let mut first = true;
        for entry in &layout {
            if let Some(rest) = entry.strip_prefix('#') {
                let (title, count) = rest.split_once('#').unwrap_or((rest, ""));
                let head = widgets::hbox(8);
                head.add_css_class("cards-group");
                if first {
                    head.add_css_class("first");
                }
                head.append(&widgets::label(&title.to_uppercase(), "cards-group-title"));
                head.append(&widgets::label(count, "cards-group-count"));
                ui.list.append(&head);
            } else if let Some(card) = cards.get(entry) {
                ui.list.append(&card.root);
            }
            first = false;
        }
        *ui.layout.borrow_mut() = layout;
    }
    let any = !ui.cards.borrow().is_empty();
    ui.list.set_visible(any);
    ui.empty.set_visible(!any);
    let narrow = window::is_narrow();
    for card in ui.cards.borrow().values() {
        apply_narrow(card, narrow);
    }
}

fn paint_chip_counts(ui: &PageUi, snapshot: &Snapshot, host: HostFilter) {
    let in_scope = |w: &Workload| match ui.scope {
        Scope::All => true,
        Scope::Vms => w.kind == Kind::VirtualMachine,
        Scope::Lxc => w.is_lxc(),
        Scope::Docker | Scope::Stacks => w.engine == Engine::Docker,
    };
    for (f, b) in ui.chip_buttons.borrow().iter() {
        let n = snapshot
            .filtered(host, None)
            .filter(|w| in_scope(w))
            .filter(|w| match f {
                Filter::All => true,
                Filter::Vms => w.kind == Kind::VirtualMachine,
                Filter::Lxc => w.is_lxc(),
                Filter::Docker => w.engine == Engine::Docker,
                Filter::Running => matches!(w.state, RunState::Running | RunState::Paused),
                Filter::Stopped => !matches!(w.state, RunState::Running | RunState::Paused),
            })
            .count();
        let n = if ui.scope == Scope::Stacks {
            let stacks = snapshot.stacks(host);
            match f {
                Filter::Running => stacks.iter().filter(|s| s.running > 0).count(),
                Filter::Stopped => stacks.iter().filter(|s| s.running == 0).count(),
                _ => stacks.len(),
            }
        } else {
            n
        };
        if let Some(count) = b.child().and_then(|c| c.last_child()).and_downcast::<gtk::Label>() {
            count.set_text(&n.to_string());
        }
    }
}

// ---------- A card ----------

fn new_card(item: &Item) -> Rc<Card> {
    let root = widgets::vbox(0);
    root.add_css_class("wl-card");
    let row = widgets::hbox(18);
    row.add_css_class("wl-row");

    let who = widgets::hbox(12);
    who.set_hexpand(true);
    who.add_css_class("wl-who");
    let tile = gtk::Overlay::new();
    let tile_box = gtk::CenterBox::new();
    tile_box.add_css_class("wl-tile");
    tile_box.set_size_request(36, 36);
    tile_box.set_valign(gtk::Align::Center);
    tile_box.set_halign(gtk::Align::Center);
    tile_box.set_hexpand(false);
    let tile_icon = gtk::Image::new();
    tile_icon.set_pixel_size(18);
    tile_box.set_center_widget(Some(&tile_icon));
    tile.set_hexpand(false);
    tile.set_child(Some(&tile_box));
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("wl-dot");
    dot.set_halign(gtk::Align::End);
    dot.set_valign(gtk::Align::End);
    tile.add_overlay(&dot);
    tile.set_valign(gtk::Align::Center);
    who.append(&tile);
    let text = widgets::vbox(3);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let name_row = widgets::hbox(7);
    let name = widgets::label("", "wl-name");
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_xalign(0.0);
    name_row.append(&name);
    let star = gtk::Image::from_icon_name("starred-symbolic");
    star.add_css_class("wl-star");
    star.set_pixel_size(12);
    name_row.append(&star);
    let tags = widgets::hbox(4);
    name_row.append(&tags);
    text.append(&name_row);
    let sub = widgets::label("", "wl-sub");
    sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
    sub.set_xalign(0.0);
    text.append(&sub);
    who.append(&text);
    row.append(&who);

    let info = widgets::vbox(5);
    info.add_css_class("wl-info");
    info.set_size_request(190, -1);
    info.set_valign(gtk::Align::Center);
    let l1 = widgets::hbox(6);
    info.append(&l1);
    let l2 = widgets::label("", "wl-l2");
    l2.set_xalign(0.0);
    l2.set_ellipsize(gtk::pango::EllipsizeMode::End);
    info.append(&l2);
    row.append(&info);

    let cpu = Meter::new("CPU");
    let mem = Meter::new("MEM");
    cpu.root.set_size_request(140, -1);
    mem.root.set_size_request(140, -1);
    cpu.root.set_valign(gtk::Align::Center);
    mem.root.set_valign(gtk::Align::Center);
    row.append(&cpu.root);
    row.append(&mem.root);
    let idle = widgets::label("", "wl-idle");
    idle.set_xalign(0.0);
    idle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    idle.set_size_request(298, -1);
    idle.set_visible(false);
    row.append(&idle);

    let acts = widgets::hbox(0);
    acts.add_css_class("wl-acts");
    acts.set_halign(gtk::Align::End);
    acts.set_valign(gtk::Align::Center);
    acts.set_size_request(196, -1);
    let acts_wrap = widgets::hbox(0);
    acts_wrap.set_halign(gtk::Align::End);
    acts_wrap.append(&acts);
    row.append(&acts_wrap);
    root.append(&row);

    let services = widgets::vbox(0);
    services.add_css_class("wl-services");
    services.set_visible(false);
    root.append(&services);

    let card = Rc::new(Card {
        root,
        tile_icon,
        dot,
        name,
        star,
        tags,
        sub,
        info,
        l1,
        l2,
        cpu,
        mem,
        idle,
        acts,
        services,
        item: RefCell::new(item.clone()),
        sig: RefCell::new(String::new()),
        svc_sig: RefCell::new(String::new()),
        svc_live: RefCell::new(Vec::new()),
        data_sig: RefCell::new(data_sig(item)),
    });
    let click = gtk::GestureClick::new();
    let weak = Rc::downgrade(&card);
    click.connect_released(move |g, n, _, _| {
        if n != 1 {
            return;
        }
        let Some(card) = weak.upgrade() else { return };
        g.set_state(gtk::EventSequenceState::Claimed);
        match &*card.item.borrow() {
            Item::One(w) => actions::open(w, Some(actions::default_view(w))),
            Item::Stack(s, _) => actions::open_stack(s, None),
        }
    });
    row.add_controller(click);
    let weak = Rc::downgrade(&card);
    nyx_ui::menu::on_right_click(&row, move |w, x, y| {
        let Some(card) = weak.upgrade() else { return };
        let entries = match &*card.item.borrow() {
            Item::One(wl) => {
                let mut e = actions::power_entries(wl);
                e.push(nyx_ui::menu::separator());
                e.extend(actions::more_entries(wl));
                e
            }
            Item::Stack(s, _) => {
                let s1 = s.clone();
                let s2 = s.clone();
                vec![
                    nyx_ui::menu::item("Open in a tab", move || actions::open_stack(&s1, None)),
                    nyx_ui::menu::item("Edit compose.yaml", move || actions::open_stack(&s2, Some("compose"))),
                ]
            }
        };
        nyx_ui::menu::popup_at(w, x, y, entries);
    });
    update_card(&card, item);
    card
}

fn set_dot(dot: &gtk::Box, state: &str) {
    for c in ["run", "paused", "err", "off"] {
        dot.remove_css_class(c);
    }
    dot.add_css_class(state);
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn port_chip(host: HostId, p: &crate::models::PortMap) -> gtk::Widget {
    let web = actions::is_web_port(p);
    let label = if p.host == p.guest || p.guest == 0 { format!(":{}", p.host) } else { format!(":{}→{}", p.host, p.guest) };
    if web {
        let b = gtk::Button::new();
        b.add_css_class("port-chip");
        b.add_css_class("link");
        let c = widgets::hbox(4);
        c.append(&widgets::label(&label, ""));
        let i = gtk::Image::from_icon_name("adw-external-link-symbolic");
        i.set_pixel_size(10);
        c.append(&i);
        b.set_child(Some(&c));
        let url = actions::port_url(host, p);
        b.set_tooltip_text(Some(&format!("Open {url}")));
        b.connect_clicked(move |_| window::open_uri(&url));
        b.upcast()
    } else {
        let l = widgets::label(&label, "port-chip");
        l.set_tooltip_text(Some(&format!("{} port {} on the host goes to {} inside", p.proto.to_uppercase(), p.host, p.guest)));
        l.upcast()
    }
}

fn icon_for(w: &Workload) -> &'static str {
    match (w.engine, w.kind) {
        (Engine::Incus, Kind::VirtualMachine) => "computer-symbolic",
        (Engine::Incus, Kind::Container) => "package-x-generic-symbolic",
        _ => "application-x-executable-symbolic",
    }
}

fn exit_code(status: &str) -> Option<i64> {
    let rest = status.strip_prefix("Exited (")?;
    rest.split(')').next()?.trim().parse().ok()
}

fn update_card(card: &Rc<Card>, item: &Item) {
    *card.item.borrow_mut() = item.clone();
    let now = crate::models::now_secs();
    match item {
        Item::One(w) => {
            card.tile_icon.set_icon_name(Some(icon_for(w)));
            let crashed = w.engine == Engine::Docker && exit_code(&w.status).is_some_and(|c| c != 0);
            let state = match w.state {
                RunState::Running => "run",
                RunState::Paused => "paused",
                _ if crashed => "err",
                RunState::Other => "err",
                _ => "off",
            };
            set_dot(&card.dot, state);
            if matches!(w.state, RunState::Running | RunState::Paused) {
                card.root.remove_css_class("off");
            } else {
                card.root.add_css_class("off");
            }
            card.name.set_text(&w.name);
            card.star.set_visible(actions::is_pinned(&w.key()));
            clear(&card.tags);
            if w.state == RunState::Paused {
                card.tags.append(&widgets::tag("Paused", "warning"));
            }
            if crashed {
                card.tags.append(&widgets::tag(&format!("Exited ({})", exit_code(&w.status).unwrap_or(1)), "danger"));
            }
            if w.host == HostId::Remote {
                card.tags.append(&widgets::tag(&window::host_name(w.host), ""));
            }
            let mut sub = vec![w.kind_tag().to_string()];
            match w.engine {
                Engine::Incus => {
                    if let Some(os) = &w.os {
                        sub.push(os.clone());
                    }
                    if let Some(c) = w.cpus {
                        sub.push(format!("{} {}", c, if w.kind == Kind::VirtualMachine { "vCPU" } else { "CPU" }));
                    }
                    if let Some(m) = w.mem_limit {
                        sub.push(short_bytes(m));
                    }
                }
                Engine::Docker => sub.push(w.image.clone().unwrap_or_default()),
            }
            let mut sub_text = sub.join(" · ");
            if window::is_narrow()
                && let Some(ip) = &w.ip
            {
                sub_text.push_str(&format!(" · {ip}"));
            }
            card.sub.set_text(&sub_text);
            card.sub.set_tooltip_text(Some(&sub_text));

            clear(&card.l1);
            if let Some(ip) = &w.ip {
                let l = widgets::label(ip, "wl-ip");
                l.set_selectable(true);
                card.l1.append(&l);
            }
            for p in w.ports.iter().take(if w.ip.is_some() { 1 } else { 2 }) {
                card.l1.append(&port_chip(w.host, p));
            }
            let shown = if w.ip.is_some() { 1 } else { 2 };
            if w.ports.len() > shown {
                let more = widgets::label(&format!("+{}", w.ports.len() - shown), "port-chip");
                let all: Vec<String> = w.ports.iter().map(|p| format!("{} → {}", p.host, p.guest)).collect();
                more.set_tooltip_text(Some(&all.join("\n")));
                card.l1.append(&more);
            }
            if w.ip.is_none() && w.ports.is_empty() && matches!(w.state, RunState::Running) {
                card.l1.append(&widgets::label("No address yet", "wl-l2"));
            }
            let mut l2 = Vec::new();
            match w.engine {
                Engine::Docker => l2.push(w.status.clone()),
                Engine::Incus => {
                    if let Some(s) = w.started {
                        l2.push(format!("Up {}", crate::models::format_uptime(s, now)));
                    }
                    if w.snapshots > 0 {
                        l2.push(format!("{} snapshot{}", w.snapshots, if w.snapshots == 1 { "" } else { "s" }));
                    }
                    if w.autostart {
                        l2.push("autostart".into());
                    }
                }
            }
            card.l2.set_text(&l2.join(" · "));

            let running = matches!(w.state, RunState::Running | RunState::Paused);
            card.cpu.root.set_visible(running);
            card.mem.root.set_visible(running);
            card.idle.set_visible(!running);
            if !running {
                let note = if crashed {
                    format!("Exited with code {}", exit_code(&w.status).unwrap_or(1))
                } else if w.state == RunState::Other {
                    w.status.clone()
                } else {
                    "Stopped".into()
                };
                card.idle.set_text(&note);
                if crashed {
                    card.idle.add_css_class("danger");
                } else {
                    card.idle.remove_css_class("danger");
                }
            }
            let sig = format!("{:?}|{}|{}|{}", w.state, actions::web_url(w).is_some(), crashed, actions::is_pinned(&w.key()));
            if *card.sig.borrow() != sig {
                *card.sig.borrow_mut() = sig;
                build_actions(card, w, crashed);
            }
            card.services.set_visible(false);
        }
        Item::Stack(stack, services) => {
            card.tile_icon.set_icon_name(Some("view-list-symbolic"));
            let state = if stack.running == stack.total && stack.total > 0 {
                "run"
            } else if stack.running > 0 {
                "paused"
            } else {
                "off"
            };
            set_dot(&card.dot, state);
            if stack.running > 0 {
                card.root.remove_css_class("off");
            } else {
                card.root.add_css_class("off");
            }
            card.name.set_text(&stack.name);
            card.star.set_visible(false);
            clear(&card.tags);
            if stack.host == HostId::Remote {
                card.tags.append(&widgets::tag(&window::host_name(stack.host), ""));
            }
            let dir = stack.project_dir.as_deref().map(tilde).unwrap_or_default();
            let sub = format!(
                "STACK · {} service{}{}",
                services.len(),
                if services.len() == 1 { "" } else { "s" },
                if dir.is_empty() { String::new() } else { format!(" · {dir}") }
            );
            card.sub.set_text(&sub);
            card.sub.set_tooltip_text(Some(&sub));
            clear(&card.l1);
            let ports: Vec<_> = services.iter().flat_map(|w| w.ports.iter().map(move |p| (w.host, p.clone()))).collect();
            for (h, p) in ports.iter().filter(|(_, p)| actions::is_web_port(p)).take(2) {
                card.l1.append(&port_chip(*h, p));
            }
            if ports.is_empty() {
                card.l1.append(&widgets::label("No published ports", "wl-l2"));
            }
            let up = services
                .iter()
                .filter_map(|w| w.started)
                .min()
                .map(|s| format!(" · up {}", crate::models::format_uptime(s, now)))
                .unwrap_or_default();
            card.l2.set_text(&format!("{} of {} running{up}", stack.running, stack.total));
            let running = stack.running > 0;
            card.cpu.root.set_visible(running);
            card.mem.root.set_visible(running);
            card.idle.set_visible(!running);
            card.idle.set_text("Stopped");
            let expanded = prefs::get().expanded.iter().any(|k| *k == item.key());
            let sig = format!("stack|{}|{}|{expanded}", stack.running, stack.total);
            if *card.sig.borrow() != sig {
                *card.sig.borrow_mut() = sig;
                build_stack_actions(card, stack, services, expanded);
            }
            card.services.set_visible(expanded);
            let svc_sig: String = services.iter().map(|w| format!("{}:{:?}:{};", w.key(), w.state, w.ports.len())).collect();
            if *card.svc_sig.borrow() != svc_sig {
                *card.svc_sig.borrow_mut() = svc_sig;
                build_services(card, services);
            }
        }
    }
    paint_live(card);
}

fn tilde(path: &str) -> String {
    let home = crate::paths::home().display().to_string();
    match path.strip_prefix(&home) {
        Some(rest) => format!("~{rest}"),
        None => path.to_string(),
    }
}

fn wide(b: &impl IsA<gtk::Widget>) {
    b.add_css_class("opt-wide");
}

fn build_actions(card: &Rc<Card>, w: &Workload, crashed: bool) {
    clear(&card.acts);
    let running = matches!(w.state, RunState::Running | RunState::Paused);
    let b = |icon: &str, tip: &str| widgets::icon_button(icon, tip);
    if !running {
        if crashed {
            let logs = b("utilities-system-monitor-symbolic", "Logs");
            let w1 = w.clone();
            logs.connect_clicked(move |_| actions::open(&w1, Some("logs")));
            card.acts.append(&logs);
            let restart = b("view-refresh-symbolic", "Restart");
            let w2 = w.clone();
            restart.connect_clicked(move |_| actions::run(&w2, crate::models::Action::Start));
            card.acts.append(&restart);
        } else {
            let start = b("media-playback-start-symbolic", "Start");
            start.add_css_class("start-button");
            let w1 = w.clone();
            start.connect_clicked(move |_| actions::run(&w1, crate::models::Action::Start));
            card.acts.append(&start);
        }
        card.acts.append(&actions::more_button(w));
        return;
    }
    match (w.engine, w.kind) {
        (Engine::Incus, Kind::VirtualMachine) => {
            let display = b("video-display-symbolic", "Open the display");
            let w1 = w.clone();
            display.connect_clicked(move |_| actions::open(&w1, Some("display")));
            card.acts.append(&display);
            let snap = b("camera-photo-symbolic", "Take a snapshot");
            wide(&snap);
            let w2 = w.clone();
            snap.connect_clicked(move |_| actions::quick_snapshot(&w2));
            card.acts.append(&snap);
            let term = b("utilities-terminal-symbolic", "Terminal");
            wide(&term);
            let w3 = w.clone();
            term.connect_clicked(move |_| actions::open(&w3, Some("terminal")));
            card.acts.append(&term);
        }
        _ => {
            if actions::web_url(w).is_some() {
                let web = b("web-browser-symbolic", "Open the web UI");
                if let Some(url) = actions::web_url(w) {
                    web.set_tooltip_text(Some(&format!("Open the web UI · {url}")));
                }
                let w1 = w.clone();
                web.connect_clicked(move |_| actions::open_web(&w1));
                card.acts.append(&web);
            }
            let term = b("utilities-terminal-symbolic", "Terminal");
            if actions::web_url(w).is_some() {
                wide(&term);
            }
            let w2 = w.clone();
            term.connect_clicked(move |_| actions::open(&w2, Some("terminal")));
            card.acts.append(&term);
            let logs = b("utilities-system-monitor-symbolic", "Logs");
            wide(&logs);
            let w3 = w.clone();
            logs.connect_clicked(move |_| actions::open(&w3, Some("logs")));
            card.acts.append(&logs);
        }
    }
    card.acts.append(&actions::power_button(w));
    card.acts.append(&actions::more_button(w));
}

fn build_stack_actions(card: &Rc<Card>, stack: &Stack, services: &[Workload], expanded: bool) {
    clear(&card.acts);
    if stack.running > 0
        && let Some(url) = actions::stack_web_url(stack, services)
    {
        let web = widgets::icon_button("web-browser-symbolic", &format!("Open the web UI · {url}"));
        web.connect_clicked(move |_| window::open_uri(&url));
        card.acts.append(&web);
    }
    let logs = widgets::icon_button("utilities-system-monitor-symbolic", "Logs");
    wide(&logs);
    let s1 = stack.clone();
    logs.connect_clicked(move |_| actions::open_stack(&s1, Some("logs")));
    card.acts.append(&logs);
    let edit = widgets::icon_button("document-edit-symbolic", "Edit compose.yaml");
    wide(&edit);
    let s2 = stack.clone();
    edit.connect_clicked(move |_| actions::open_stack(&s2, Some("compose")));
    card.acts.append(&edit);
    card.acts.append(&actions::stack_power_button(stack));
    let toggle = widgets::icon_button(if expanded { "pan-up-symbolic" } else { "pan-down-symbolic" }, if expanded {
        "Hide the services"
    } else {
        "Show the services"
    });
    let key = format!("stack|{}|{}", stack.host.label(), stack.name);
    toggle.connect_clicked(move |_| {
        let k = key.clone();
        prefs::update(|p| {
            if let Some(i) = p.expanded.iter().position(|x| *x == k) {
                p.expanded.remove(i);
            } else {
                p.expanded.push(k.clone());
            }
        });
        let k = key.clone();
        glib::idle_add_local_once(move || {
            let _ = k;
            if let Some(snap) = window::snapshot() {
                refresh(&snap, window::host_filter(), "");
            }
        });
    });
    card.acts.append(&toggle);
}

fn build_services(card: &Rc<Card>, services: &[Workload]) {
    clear(&card.services);
    let mut live = Vec::new();
    for w in services {
        let row = widgets::hbox(18);
        row.add_css_class("svc-row");
        let who = widgets::hbox(10);
        who.set_hexpand(true);
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("svc-dot");
        dot.add_css_class(match w.state {
            RunState::Running => "run",
            RunState::Paused => "paused",
            _ if exit_code(&w.status).is_some_and(|c| c != 0) => "err",
            _ => "off",
        });
        dot.set_valign(gtk::Align::Center);
        who.append(&dot);
        let name = widgets::label(w.service.as_deref().unwrap_or(&w.name), "svc-name");
        who.append(&name);
        let image = widgets::label(w.image.as_deref().unwrap_or(""), "svc-image");
        image.set_ellipsize(gtk::pango::EllipsizeMode::End);
        image.set_xalign(0.0);
        image.set_hexpand(true);
        who.append(&image);
        row.append(&who);
        let info = widgets::hbox(6);
        info.add_css_class("wl-info");
        info.set_size_request(190, -1);
        for p in w.ports.iter().take(2) {
            info.append(&port_chip(w.host, p));
        }
        if w.ports.is_empty() {
            info.append(&widgets::label(if w.state == RunState::Running { "no ports" } else { &w.status }, "wl-l2"));
        }
        row.append(&info);
        let cpu = widgets::label("—", "svc-num");
        cpu.set_size_request(140, -1);
        cpu.set_xalign(0.0);
        row.append(&cpu);
        let mem = widgets::label("—", "svc-num");
        mem.set_size_request(140, -1);
        mem.set_xalign(0.0);
        row.append(&mem);
        let acts = widgets::hbox(0);
        acts.set_size_request(196, -1);
        acts.set_halign(gtk::Align::End);
        let logs = widgets::icon_button("utilities-system-monitor-symbolic", "Logs");
        let w1 = w.clone();
        logs.connect_clicked(move |_| actions::open(&w1, Some("logs")));
        let term = widgets::icon_button("utilities-terminal-symbolic", "Terminal");
        let w2 = w.clone();
        term.connect_clicked(move |_| actions::open(&w2, Some("terminal")));
        let restart = widgets::icon_button("view-refresh-symbolic", "Restart");
        let w3 = w.clone();
        restart.connect_clicked(move |_| actions::run(&w3, crate::models::Action::Restart));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        acts.append(&spacer);
        acts.append(&logs);
        acts.append(&term);
        acts.append(&restart);
        acts.append(&actions::more_button(w));
        row.append(&acts);
        let click = gtk::GestureClick::new();
        let w4 = w.clone();
        click.connect_released(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            actions::open(&w4, None);
        });
        row.add_controller(click);
        card.services.append(&row);
        live.push((w.key(), cpu, mem));
    }
    *card.svc_live.borrow_mut() = live;
}

fn paint_live(card: &Card) {
    let item = card.item.borrow();
    let (keys, limit) = match &*item {
        Item::One(w) => (vec![w.key()], w.mem_limit),
        Item::Stack(_, services) => (services.iter().map(Workload::key).collect(), None),
    };
    let (cpu_vals, mem_vals) = if keys.len() == 1 {
        history::get(&keys[0]).map(|h| (h.cpu.iter().copied().collect::<Vec<_>>(), h.mem.iter().copied().collect::<Vec<_>>())).unwrap_or_default()
    } else {
        history::total_series(&keys)
    };
    let last = keys.first().and_then(|k| history::get(k)).and_then(|h| h.last);
    let limit = limit.or(last.as_ref().and_then(|l| l.mem_limit)).filter(|l| *l > 0);
    let host_mem = HEALTH.with(|h| h.borrow().iter().find_map(|x| x.mem_total));
    // Docker reports the whole host as the limit when none is set: don't draw that.
    let limit = match (&*item, limit, host_mem) {
        (Item::One(w), Some(l), Some(total)) if w.engine == Engine::Docker && l as f64 >= total as f64 * 0.95 => None,
        (Item::Stack(..), _, _) => None,
        (_, l, _) => l,
    };
    let cpu_now = cpu_vals.last().copied();
    // Like Proxmox, the CPU graph scales to its recent peak so small loads still show a shape.
    let cpu_scale = (cpu_vals.iter().copied().fold(0.0, f64::max) * 1.25).clamp(5.0, 100.0);
    card.cpu.set(
        &cpu_now.map(|c| if c < 10.0 { format!("{c:.1}%") } else { format!("{c:.0}%") }).unwrap_or_else(|| "—".into()),
        cpu_vals.iter().map(|v| v / cpu_scale).collect(),
        None,
    );
    let mem_now = mem_vals.last().copied();
    let scale = limit.map(|l| l as f64).unwrap_or_else(|| mem_vals.iter().copied().max().unwrap_or(1) as f64 * 1.25).max(1.0);
    card.mem.set(
        &mem_text(mem_now, limit),
        mem_vals.iter().map(|v| *v as f64 / scale).collect(),
        limit.map(|_| 0.92),
    );
    for (key, cpu, mem) in card.svc_live.borrow().iter() {
        let h = history::get(key);
        let last = h.as_ref().and_then(|h| h.last.clone());
        cpu.set_text(&format!(
            "CPU  {}",
            last.as_ref().and_then(|l| l.cpu).map(|c| format!("{c:.1}%")).unwrap_or_else(|| "—".into())
        ));
        mem.set_text(&format!("MEM  {}", last.as_ref().and_then(|l| l.mem).map(short_bytes).unwrap_or_else(|| "—".into())));
    }
}

// ---------- The host strip on the Overview ----------

struct HostStrip {
    root: gtk::Box,
    cpu: Meter,
    mem: Meter,
    disk: Meter,
    guests: Meter,
    engines: gtk::Box,
}

impl HostStrip {
    fn new() -> HostStrip {
        let root = widgets::hbox(0);
        root.add_css_class("host-strip");
        let cpu = Meter::new("CPU");
        let mem = Meter::new("MEMORY");
        let disk = Meter::new("ENGINE DISK");
        let guests = Meter::new("RUNNING");
        for m in [&cpu, &mem, &disk, &guests] {
            m.root.add_css_class("host-meter");
            m.root.set_hexpand(true);
            root.append(&m.root);
        }
        let engines = widgets::vbox(6);
        engines.add_css_class("host-engines");
        engines.set_valign(gtk::Align::Center);
        root.append(&engines);
        HostStrip { root, cpu, mem, disk, guests, engines }
    }

    fn paint(&self, filter: HostFilter, snap: &Snapshot) {
        let host = if filter == HostFilter::Remote { HostId::Remote } else { HostId::Local };
        let health = HEALTH.with(|h| h.borrow().iter().find(|x| x.host == host).cloned());
        match &health {
            Some(h) => {
                self.cpu.set(
                    &h.cpu.map(|c| format!("{c:.0}%")).unwrap_or_else(|| "—".into()),
                    h.cpu_hist.clone(),
                    None,
                );
                self.mem.set(&mem_text(h.mem_used, h.mem_total), h.mem_hist.clone(), None);
                let free = match (h.disk_used, h.disk_total) {
                    (Some(u), Some(t)) if t >= u => format!("{} free", short_bytes(t - u)),
                    (Some(u), _) => format!("{} used", short_bytes(u)),
                    _ => "—".into(),
                };
                self.disk.set(&free, h.disk_hist.clone(), None);
            }
            None => {
                self.cpu.set("—", Vec::new(), None);
                self.mem.set("—", Vec::new(), None);
                self.disk.set("—", Vec::new(), None);
            }
        }
        let total = snap.filtered(filter, None).count();
        let running = snap.filtered(filter, None).filter(|w| w.state == RunState::Running).count();
        let frac = if total == 0 { 0.0 } else { running as f64 / total as f64 };
        self.guests.set(&format!("{running} of {total}"), vec![frac; history::POINTS], None);
        clear(&self.engines);
        for st in snap.engines.iter().filter(|e| filter.matches(e.host)) {
            let row = widgets::hbox(7);
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("svc-dot");
            dot.add_css_class(if st.is_connected() { "run" } else { "err" });
            dot.set_valign(gtk::Align::Center);
            row.append(&dot);
            let detail = match &st.state {
                crate::models::ConnState::Connected { detail } if detail.starts_with(st.engine.label()) => detail.clone(),
                crate::models::ConnState::Connected { detail } => format!("{} {detail}", st.engine.label()),
                _ => format!("{} unavailable", st.engine.label()),
            };
            let detail = if filter == HostFilter::All && st.host == HostId::Remote {
                format!("{detail} · {}", window::host_name(st.host))
            } else {
                detail
            };
            let l = widgets::label(&detail, "host-engine");
            row.append(&l);
            if !st.is_connected() {
                let fix = gtk::Button::with_label("Fix");
                fix.add_css_class("flat");
                fix.add_css_class("host-fix");
                fix.connect_clicked(|_| crate::settings_dialog::open_diagnostics());
                row.append(&fix);
            }
            self.engines.append(&row);
        }
    }
}
