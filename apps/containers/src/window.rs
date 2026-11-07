//! The main window: the Nyx 2.0 header (the app and where you are; search in the
//! middle; the host filter, refresh, New and Settings at the end), the sidebar of
//! pages, and a tab row. Tab 0 holds the sidebar's page; every workload or stack
//! you open gets a tab of its own, VMware Workstation style.

use crate::models::{
    DiagReport, HostFilter, HostHealth, HostId, ImageDetail, InstanceSnap, LiveSample, LogChunk, Snapshot,
    WorkloadDetail,
};
use crate::sections::{self, Section};
use crate::tabs::{self, Target};
use crate::{history, prefs, settings_dialog, theme, widgets, worker};
use gtk::prelude::*;
use gtk::{gdk, glib};
use nyx_ui::chrome;
use nyx_ui::sidebar::{NavItem, Sidebar};
use nyx_ui::tabs::{Tab, TabRow};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

struct OpenTab {
    target: Target,
    key: String,
}

struct Ui {
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    header: chrome::AppHeader,
    sidebar: Rc<Sidebar>,
    nav: HashMap<&'static str, NavItem>,
    tab_row: Rc<TabRow>,
    /// "home" (the sidebar's pages) and one child per open tab.
    main: gtk::Stack,
    home: gtk::Stack,
    pages: HashMap<String, gtk::Widget>,
    sections: Vec<Section>,
    current: String,
    tabs: Vec<OpenTab>,
    /// 0 is the sidebar's page; 1.. index `tabs` from one.
    active: usize,
    host: HostFilter,
    host_button: gtk::Button,
    search: gtk::SearchEntry,
    snapshot: Snapshot,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
    static NARROW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PENDING_OPEN: RefCell<Option<(String, Option<String>)>> = const { RefCell::new(None) };
}

/// Open a workload or stack by name; waits for the first inventory if needed.
pub fn open_by_name(name: &str, view: Option<&str>) {
    let found = snapshot().and_then(|snap| {
        if let Some(w) = snap.workloads.iter().find(|w| w.name == name) {
            return Some(Target::Workload { key: w.key() });
        }
        snap.stacks(HostFilter::All).into_iter().find(|s| s.name == name).map(|s| Target::Stack { host: s.host, project: s.name })
    });
    match found {
        Some(target) => open_tab(target, view),
        None => PENDING_OPEN.with(|p| *p.borrow_mut() = Some((name.to_string(), view.map(String::from)))),
    }
}

fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    ui().map(|u| u.borrow().window.clone())
}

pub fn overlay() -> Option<gtk::Overlay> {
    ui().map(|u| u.borrow().overlay.clone())
}

pub fn present(app: &gtk::Application, section: Option<&str>) {
    if let Some(ui) = ui() {
        let window = ui.borrow().window.clone();
        if let Some(s) = section {
            navigate(s);
        }
        window.present();
        return;
    }
    theme::install();
    install_icons();
    build(app);
    let start = section.map(String::from).unwrap_or_else(|| nyx_theme::start_page("overview", &prefs::get().last_section));
    navigate(&start);
    if prefs::take_broken() {
        toast("Your settings file couldn't be read, so defaults are in use. The old file is kept as settings.toml.bak.");
    }
    worker::start();
    if let Some(ui) = ui() {
        ui.borrow().window.present();
    }
}

fn install_icons() {
    macro_rules! icon {
        ($name:literal) => {
            ($name, include_str!(concat!("../data/icons/", $name)))
        };
    }
    const ICONS: &[(&str, &str)] = &[
        icon!("io.github.design_nexus.nyx.Containers.svg"),
        icon!("nyx-os-ubuntu.svg"),
        icon!("nyx-os-debian.svg"),
        icon!("nyx-os-fedora.svg"),
        icon!("nyx-os-archlinux.svg"),
        icon!("nyx-os-nixos.svg"),
        icon!("nyx-os-opensuse.svg"),
        icon!("nyx-os-alpine.svg"),
        icon!("nyx-os-rockylinux.svg"),
        icon!("nyx-os-windows.svg"),
        icon!("nyx-os-iso.svg"),
    ];
    let dir = crate::paths::cache_dir().join("icons");
    for (name, svg) in ICONS {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = crate::cmd::atomic_write(&path, svg);
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

fn build(app: &gtk::Application) {
    let window =
        gtk::ApplicationWindow::builder().application(app).title("Containers").default_width(1280).default_height(800).build();
    window.add_css_class("containers-window");
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    nyx_ui::keys::install(
        &window,
        nyx_ui::keys::Common { settings: Some(settings_dialog::open), shortcuts: Some(show_shortcuts), ..Default::default() },
    );
    window.set_icon_name(Some("io.github.design_nexus.nyx.Containers"));

    let sections = sections::all();
    let sidebar = Sidebar::new();
    let mut nav = HashMap::new();
    let mut last_group = "\u{0}";
    for s in sections.iter().filter(|s| s.nav) {
        if s.group != last_group {
            let first = last_group == "\u{0}";
            if !s.group.is_empty() {
                let label = match s.group {
                    "Workloads" => nyx_ui::sidebar::group_label_with_add(s.group, first, "New workload (Ctrl+N)", || {
                        crate::sections::create::open(None)
                    }),
                    "Hosts" => nyx_ui::sidebar::group_label_with_add(s.group, first, "Add a remote host", || {
                        navigate("connections")
                    }),
                    _ => nyx_ui::sidebar::group_label(s.group, first),
                };
                if let Some(heading) = label.first_child() {
                    heading.add_css_class(&format!("section-{}", s.group.to_ascii_lowercase()));
                }
                sidebar.append(&label);
            }
            last_group = s.group;
        }
        let item = nyx_ui::sidebar::nav_item(s.icon, s.title, s.description);
        let id = s.id;
        item.button.connect_clicked(move |_| navigate(id));
        sidebar.append(&item.button);
        nav.insert(s.id, item);
    }
    sidebar.follow_window(&window);
    sidebar.set_collapsed(prefs::get().sidebar_collapsed);

    let home = gtk::Stack::new();
    home.set_hexpand(true);
    home.set_vexpand(true);
    home.set_transition_type(gtk::StackTransitionType::Crossfade);
    home.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });

    let main = gtk::Stack::new();
    main.add_css_class("ops-stack");
    main.set_hexpand(true);
    main.set_vexpand(true);
    main.add_named(&home, Some("home"));

    let tab_row = TabRow::new("New workload (Ctrl+N)");
    tab_row.connect_new(|| crate::sections::create::open(None));
    tab_row.connect_select(select_tab);
    tab_row.connect_close(|i| close_tab(i));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_hexpand(true);
    column.add_css_class("ops-chrome");
    column.append(&tab_row.root);
    column.append(&main);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(&sidebar.root);
    body.append(&column);

    let header = chrome::app_header(&window, "package-x-generic-symbolic", "Containers");
    let collapse = widgets::bar_button("sidebar-show-symbolic", "Collapse the sidebar (Ctrl+B)");
    collapse.connect_clicked(|_| toggle_sidebar());
    header.start.append(&collapse);
    let search = search_entry();
    header.center.append(&search);
    header.end.set_spacing(4);
    let host_button = host_menu_button();
    header.end.append(&host_button);
    let refresh = widgets::bar_button("view-refresh-symbolic", "Refresh (Ctrl+R)");
    refresh.connect_clicked(|_| worker::refresh());
    header.end.append(&refresh);
    let new = widgets::primary_button("list-add-symbolic", "New workload (Ctrl+N)");
    new.connect_clicked(|_| crate::sections::create::open(None));
    header.end.append(&new);
    header.end.append(&chrome::settings_button(settings_dialog::open));

    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("window-frame");
    frame.append(&header.root);
    frame.append(&body);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&frame));
    window.set_child(Some(&overlay));

    install_keys(&window, &search);
    let last = std::cell::Cell::new(0);
    window.add_tick_callback(move |w, _| {
        if w.width() != last.get() {
            last.set(w.width());
            let narrow = w.width() > 0 && w.width() < 900;
            if narrow != NARROW.with(|n| n.get()) {
                NARROW.with(|n| n.set(narrow));
                crate::sections::cards::set_narrow(narrow);
                glib::idle_add_local_once(repaint);
                if let Some(ui) = ui() {
                    ui.borrow().search.set_width_chars(if narrow { 14 } else { 30 });
                }
            }
            settings_dialog::fit(w);
        }
        glib::ControlFlow::Continue
    });

    let ui = Ui {
        window,
        overlay,
        header,
        sidebar,
        nav,
        tab_row,
        main,
        home,
        pages: HashMap::new(),
        sections,
        current: String::new(),
        tabs: Vec::new(),
        active: 0,
        host: HostFilter::All,
        host_button,
        search,
        snapshot: Snapshot::default(),
    };
    UI.with(|u| *u.borrow_mut() = Some(Rc::new(RefCell::new(ui))));
}

fn search_entry() -> gtk::SearchEntry {
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search workloads, images, stacks"));
    search.add_css_class("bar-search");
    search.set_width_chars(30);
    search.update_property(&[gtk::accessible::Property::Label("Search")]);
    search.connect_search_changed(|_| paint_lists());
    search.connect_stop_search(|e| e.set_text(""));
    search
}

fn host_menu_button() -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("host-menu");
    let content = widgets::hbox(8);
    content.append(&gtk::Image::from_icon_name("computer-symbolic"));
    content.append(&widgets::label("All hosts", "host-menu-label"));
    content.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
    button.set_child(Some(&content));
    button.set_tooltip_text(Some("Which hosts to show"));
    button.set_valign(gtk::Align::Center);
    button.connect_clicked(|b| {
        let current = host_filter();
        let remote = prefs::get().remote;
        let remote_name = if remote.configured() { remote.display_name() } else { "Remote".into() };
        let entries = vec![
            nyx_ui::menu::check("All hosts", current == HostFilter::All, |_| set_host(HostFilter::All)),
            nyx_ui::menu::check("This computer", current == HostFilter::Local, |_| set_host(HostFilter::Local)),
            nyx_ui::menu::check(&remote_name, current == HostFilter::Remote, |_| set_host(HostFilter::Remote)),
            nyx_ui::menu::separator(),
            nyx_ui::menu::item("Connections…", || navigate("connections")).icon("network-server-symbolic"),
        ];
        nyx_ui::menu::popup_below(b, entries);
    });
    button
}

pub fn set_host(filter: HostFilter) {
    let Some(ui) = ui() else { return };
    if filter == HostFilter::Remote && !crate::services::connections::remote_ready(&prefs::get().remote) {
        toast("Add a remote on Connections.");
        navigate("connections");
        return;
    }
    {
        let mut u = ui.borrow_mut();
        u.host = filter;
        let text = match filter {
            HostFilter::All => "All hosts".to_string(),
            HostFilter::Local => "This computer".to_string(),
            HostFilter::Remote => prefs::get().remote.display_name(),
        };
        if let Some(label) = find_label(u.host_button.upcast_ref(), "host-menu-label") {
            label.set_text(&text);
        }
    }
    paint_lists();
}

fn find_label(root: &gtk::Widget, class: &str) -> Option<gtk::Label> {
    if root.has_css_class(class) {
        return root.downcast_ref::<gtk::Label>().cloned();
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        if let Some(l) = find_label(&c, class) {
            return Some(l);
        }
        child = c.next_sibling();
    }
    None
}

pub fn is_narrow() -> bool {
    NARROW.with(|n| n.get())
}

/// A VM display takes the whole screen: the window goes fullscreen and its header,
/// sidebar, tabs and toolbars hide.
pub fn set_display_fullscreen(on: bool) {
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    u.header.root.set_visible(!on);
    u.sidebar.root.set_visible(!on);
    u.tab_row.root.set_visible(!on);
    tabs::set_toolbars_visible(!on);
    if on {
        u.window.fullscreen();
    } else {
        u.window.unfullscreen();
    }
}

pub fn toggle_sidebar() {
    prefs::update(|p| p.sidebar_collapsed = !p.sidebar_collapsed);
    let Some(ui) = ui() else { return };
    let sidebar = ui.borrow().sidebar.clone();
    sidebar.set_collapsed(prefs::get().sidebar_collapsed);
}

fn install_keys(window: &gtk::ApplicationWindow, search: &gtk::SearchEntry) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let s2 = search.clone();
    let w2 = window.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let typing = gtk::prelude::GtkWindowExt::focus(&w2).is_some_and(|f| {
            f.is::<gtk::Text>() || f.ancestor(gtk::Entry::static_type()).is_some() || f.is::<gtk::SearchEntry>()
        });
        // A terminal or VM display owns every key while it has focus.
        if crate::tabs::focus_grabbed(&w2) && !(ctrl && shift) {
            return glib::Propagation::Proceed;
        }
        let done = glib::Propagation::Stop;
        if settings_dialog::is_open() {
            return match key {
                gdk::Key::Escape => {
                    settings_dialog::escape();
                    done
                }
                gdk::Key::f if ctrl => {
                    settings_dialog::focus_search();
                    done
                }
                gdk::Key::q if ctrl => {
                    w2.close();
                    done
                }
                _ => glib::Propagation::Proceed,
            };
        }
        match key {
            gdk::Key::f if ctrl => {
                s2.grab_focus();
                done
            }
            gdk::Key::F1 => {
                show_shortcuts();
                done
            }
            gdk::Key::b if ctrl => {
                toggle_sidebar();
                done
            }
            gdk::Key::n if ctrl => {
                crate::sections::create::open(None);
                done
            }
            gdk::Key::q if ctrl => {
                w2.close();
                done
            }
            gdk::Key::w | gdk::Key::W if ctrl => {
                let active = ui().map(|u| u.borrow().active).unwrap_or(0);
                if active > 0 {
                    close_tab(active);
                } else {
                    w2.close();
                }
                done
            }
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab if ctrl => {
                cycle_tab(if shift || key == gdk::Key::ISO_Left_Tab { -1 } else { 1 });
                done
            }
            gdk::Key::Page_Down if ctrl => {
                cycle_tab(1);
                done
            }
            gdk::Key::Page_Up if ctrl => {
                cycle_tab(-1);
                done
            }
            k if ctrl && (gdk::Key::_1..=gdk::Key::_9).contains(&k) => {
                let n = (k.into_glib() - gdk::Key::_1.into_glib()) as usize;
                select_tab(n);
                done
            }
            gdk::Key::question if !ctrl && !typing => {
                show_shortcuts();
                done
            }
            gdk::Key::r if ctrl => {
                worker::refresh();
                done
            }
            gdk::Key::Escape if !s2.text().is_empty() => {
                s2.set_text("");
                done
            }
            _ => glib::Propagation::Proceed,
        }
    });
    window.add_controller(keys);
}

use gtk::glib::translate::IntoGlib;

/// Show a sidebar page (in tab 0). Old ids keep working.
pub fn navigate(id: &str) {
    let id = match id {
        "containers" => "docker",
        "host-all" | "host-local" => "host",
        "host-remote" => "homelab",
        "settings" => {
            settings_dialog::open();
            return;
        }
        other => other,
    };
    let Some(ui) = ui() else { return };
    let missing = {
        let u = ui.borrow();
        if u.pages.contains_key(id) { None } else { u.sections.iter().find(|s| s.id == id).cloned().map(Some).unwrap_or(None) }
    };
    if !ui.borrow().pages.contains_key(id) && missing.is_none() {
        return;
    }
    // Build with nothing borrowed: a page may read the window's state while it builds.
    if let Some(section) = missing {
        let page = widgets::page(section.id);
        if section.fill {
            page.fill();
        }
        (section.build)(&page);
        let mut u = ui.borrow_mut();
        u.home.add_named(&page.root, Some(section.id));
        u.pages.insert(id.to_string(), page.root.clone().upcast());
    }
    {
        let mut u = ui.borrow_mut();
        u.home.set_visible_child_name(id);
        u.current = id.to_string();
        if id != "connections" {
            prefs::update(|p| p.last_section = id.to_string());
        }
    }
    select_tab(0);
    paint_lists();
}

fn section_of(u: &Ui, id: &str) -> Option<Section> {
    u.sections.iter().find(|s| s.id == id).cloned()
}

/// Re-draw the tab row from the open tabs.
fn paint_tabs(ui: &Rc<RefCell<Ui>>) {
    let (row, tabs, active) = {
        let u = ui.borrow();
        let section = section_of(&u, &u.current);
        let mut tabs = vec![Tab {
            title: section.as_ref().map(|s| s.title.to_string()).unwrap_or_else(|| "Overview".into()),
            icon: Some(section.as_ref().map(|s| s.icon).unwrap_or("view-grid-symbolic").to_string()),
            tooltip: Some("The sidebar's page".into()),
            dirty: false,
        }];
        for t in &u.tabs {
            let info = tabs::info(&t.target, &u.snapshot);
            tabs.push(Tab { title: info.title, icon: Some(info.icon.to_string()), tooltip: Some(info.tooltip), dirty: false });
        }
        (u.tab_row.clone(), tabs, u.active)
    };
    row.set_tabs(&tabs, active);
}

fn select_tab(index: usize) {
    let Some(ui) = ui() else { return };
    {
        let mut u = ui.borrow_mut();
        let index = index.min(u.tabs.len());
        u.active = index;
        if index == 0 {
            u.main.set_visible_child_name("home");
            let title = section_of(&u, &u.current).map(|s| s.title).unwrap_or("Overview");
            chrome::set_title(&u.window, "Containers", title);
            u.header.set_subtitle(title);
        } else {
            let key = u.tabs[index - 1].key.clone();
            u.main.set_visible_child_name(&key);
            let info = tabs::info(&u.tabs[index - 1].target, &u.snapshot);
            chrome::set_title(&u.window, "Containers", &info.title);
            u.header.set_subtitle(&info.title);
        }
    }
    highlight_nav(&ui);
    paint_tabs(&ui);
}

fn cycle_tab(step: i32) {
    let Some(ui) = ui() else { return };
    let (active, count) = {
        let u = ui.borrow();
        (u.active as i32, u.tabs.len() as i32 + 1)
    };
    select_tab(((active + step).rem_euclid(count)) as usize);
}

/// Open (or switch to) the tab for `target`, on `view` when given.
pub fn open_tab(target: Target, view: Option<&str>) {
    let Some(ui) = ui() else { return };
    let key = target.key();
    let existing = ui.borrow().tabs.iter().position(|t| t.key == key);
    let index = match existing {
        Some(i) => i + 1,
        None => {
            let snapshot = ui.borrow().snapshot.clone();
            let widget = tabs::build(&target, &snapshot);
            let mut u = ui.borrow_mut();
            u.main.add_named(&widget, Some(&key));
            u.tabs.push(OpenTab { target, key: key.clone() });
            u.tabs.len()
        }
    };
    select_tab(index);
    if let Some(view) = view {
        tabs::show_view(&key, view);
    }
}

fn close_tab(index: usize) {
    let Some(ui) = ui() else { return };
    if index == 0 {
        return;
    }
    let key = {
        let mut u = ui.borrow_mut();
        if index > u.tabs.len() {
            return;
        }
        u.tabs.remove(index - 1).key
    };
    tabs::closed(&key);
    {
        let mut u = ui.borrow_mut();
        if let Some(child) = u.main.child_by_name(&key) {
            u.main.remove(&child);
        }
        if u.active == index {
            u.active = if index <= u.tabs.len() { index } else { index - 1 };
        } else if u.active > index {
            u.active -= 1;
        }
    }
    let active = ui.borrow().active;
    select_tab(active);
}

fn highlight_nav(ui: &Rc<RefCell<Ui>>) {
    let u = ui.borrow();
    for (id, item) in &u.nav {
        item.set_active(u.active == 0 && *id == u.current);
    }
}

fn paint_counts(u: &Ui) {
    let snap = &u.snapshot;
    let host = u.host;
    let count = |f: &dyn Fn(&crate::models::Workload) -> bool| snap.filtered(host, None).filter(|w| f(w)).count() as u64;
    let set = |id: &str, n: u64| {
        if let Some(item) = u.nav.get(id) {
            item.set_count(Some(n));
        }
    };
    set("overview", snap.filtered(host, None).count() as u64);
    set("vms", count(&|w| w.kind == crate::models::Kind::VirtualMachine));
    set("lxc", count(&|w| w.is_lxc()));
    set("docker", count(&|w| w.engine == crate::models::Engine::Docker));
    set("stacks", snap.stacks(host).len() as u64);
    set("images", snap.images(host).count() as u64);
    set("storage", snap.volumes(host).count() as u64);
    set("networks", snap.networks(host).count() as u64);
    if let Some(item) = u.nav.get("homelab") {
        let remote = prefs::get().remote;
        let name = if remote.configured() { remote.display_name() } else { "Add a remote".into() };
        item.set_title(&name);
    }
}

/// Repaint every page from the current inventory.
pub fn repaint() {
    paint_lists();
}

fn paint_lists() {
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    let query = u.search.text().to_string();
    paint_counts(&u);
    crate::sections::cards::refresh(&u.snapshot, u.host, &query);
    crate::sections::host::refresh(&u.snapshot);
    crate::sections::images::refresh(&u.snapshot, u.host, &query);
    crate::sections::networks::refresh(&u.snapshot, u.host, &query);
    crate::sections::storage::refresh(&u.snapshot, u.host, &query);
    crate::sections::connections::refresh_local(&u.snapshot);
}

pub fn host_name(host: HostId) -> String {
    match host {
        HostId::Local => "This computer".into(),
        HostId::Remote => {
            let n = prefs::get().remote.display_name();
            if n.is_empty() { "Remote".into() } else { n }
        }
    }
}

/// The address a workload's published ports answer on, as seen from this computer.
pub fn host_address(host: HostId) -> String {
    match host {
        HostId::Local => "localhost".into(),
        HostId::Remote => prefs::get().remote.host.trim().to_string(),
    }
}

pub fn host_filter() -> HostFilter {
    ui().map(|u| u.borrow().host).unwrap_or(HostFilter::All)
}

pub fn snapshot() -> Option<Snapshot> {
    ui().map(|u| u.borrow().snapshot.clone())
}

pub fn apply_health(list: Vec<HostHealth>) {
    crate::sections::host::apply_health(list.clone());
    crate::sections::cards::apply_health(list);
}

pub fn apply_live(samples: Vec<LiveSample>) {
    history::push(&samples);
    crate::sections::cards::apply_live();
    tabs::apply_live();
}

pub fn apply_diag(report: DiagReport) {
    crate::sections::diagnose::apply(report);
}

pub fn apply_fix(result: Result<String, String>) {
    match result {
        Ok(msg) => {
            toast(&msg);
            worker::diagnose();
            worker::refresh();
        }
        Err(err) => {
            if let Some(cmd) = err.lines().last() {
                copy_text(cmd);
            }
            toast(&err);
        }
    }
}

pub fn copy_text(text: &str) {
    if let Some(display) = gdk::Display::default() {
        display.clipboard().set_text(text);
    }
}

pub fn open_uri(uri: &str) {
    let launcher = gtk::UriLauncher::new(uri);
    launcher.launch(window().as_ref(), gtk::gio::Cancellable::NONE, |r| {
        if let Err(e) = r {
            toast(&format!("Couldn't open the link: {e}"));
        }
    });
}

pub fn apply_snapshot(snapshot: Snapshot) {
    let Some(ui) = ui() else { return };
    {
        let mut u = ui.borrow_mut();
        u.snapshot = snapshot;
        let remote_ok = crate::services::connections::remote_ready(&prefs::get().remote);
        if !remote_ok && u.host == HostFilter::Remote {
            u.host = HostFilter::All;
        }
    }
    paint_lists();
    let snap = ui.borrow().snapshot.clone();
    tabs::refresh(&snap);
    paint_tabs(&ui);
    if let Some((name, view)) = PENDING_OPEN.with(|p| p.borrow_mut().take()) {
        open_by_name(&name, view.as_deref());
    }
}

pub fn apply_op(result: Result<String, String>) {
    match result {
        Ok(msg) => {
            toast(&msg);
            worker::refresh();
        }
        Err(err) => toast(&err),
    }
}

pub fn apply_detail(result: Result<WorkloadDetail, String>) {
    match result {
        Ok(detail) => tabs::apply_detail(detail),
        Err(err) => toast(&err),
    }
}

pub fn apply_image_detail(result: Result<ImageDetail, String>) {
    match result {
        Ok(detail) => crate::sections::images::show_detail(detail),
        Err(err) => toast(&err),
    }
}

pub fn apply_snaps(host: HostId, instance: String, result: Result<Vec<InstanceSnap>, String>) {
    match result {
        Ok(snaps) => tabs::apply_snaps(host, &instance, &snaps),
        Err(err) => toast(&err),
    }
}

pub fn apply_log(chunk: LogChunk) {
    crate::sections::session::apply_chunk(chunk);
}

pub fn apply_log_done(seq: u64, result: Result<(), String>) {
    crate::sections::session::apply_done(seq, result);
}

pub fn apply_term_bytes(seq: u64, bytes: Vec<u8>) {
    crate::sections::session::apply_bytes(seq, bytes);
}

pub fn apply_term_done(seq: u64, result: Result<(), String>) {
    crate::sections::session::apply_term_done(seq, result);
}

pub fn apply_test(results: Vec<crate::models::EngineStatus>) {
    crate::sections::connections::show_test(&results);
    let ok = results.iter().filter(|e| e.is_connected()).count();
    let name = crate::services::connections::remote_label(&prefs::get().remote);
    toast(&format!("{name}: {ok} of {} engines connected.", results.len()));
}

/// A card over the window on a dimmed backdrop, like the settings card. Returns the
/// card to fill and a function that closes it; Esc or a click outside also closes.
pub fn overlay_card(title: &str, width: i32) -> (gtk::Box, Rc<dyn Fn()>) {
    let veil = gtk::Box::new(gtk::Orientation::Vertical, 0);
    veil.add_css_class("card-veil");
    veil.set_hexpand(true);
    veil.set_vexpand(true);
    veil.set_focusable(true);
    let card = widgets::vbox(14);
    card.add_css_class("overlay-card");
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    card.set_vexpand(true);
    card.set_size_request(width, -1);
    let head = widgets::hbox(8);
    let t = widgets::label(title, "overlay-card-title");
    t.set_hexpand(true);
    t.set_xalign(0.0);
    head.append(&t);
    let x = widgets::icon_button("window-close-symbolic", "Close (Esc)");
    head.append(&x);
    card.append(&head);
    veil.append(&card);
    let overlay = overlay();
    let close: Rc<dyn Fn()> = {
        let (veil, overlay) = (veil.clone(), overlay.clone());
        Rc::new(move || {
            if let Some(o) = &overlay
                && veil.parent().is_some()
            {
                o.remove_overlay(&veil);
            }
        })
    };
    let c = close.clone();
    x.connect_clicked(move |_| c());
    let keys = gtk::EventControllerKey::new();
    let c = close.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            c();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    veil.add_controller(keys);
    let click = gtk::GestureClick::new();
    let (c, card_ref) = (close.clone(), card.clone());
    click.connect_released(move |g, _, x, y| {
        let Some(v) = g.widget() else { return };
        let inside = card_ref.compute_bounds(&v).is_some_and(|b| b.contains_point(&gtk::graphene::Point::new(x as f32, y as f32)));
        if !inside {
            c();
        }
    });
    veil.add_controller(click);
    if let Some(o) = overlay {
        o.add_overlay(&veil);
        veil.grab_focus();
    }
    (card, close)
}

pub fn toast(message: &str) {
    match overlay() {
        Some(overlay) => nyx_ui::toast::show(&overlay, message),
        None => eprintln!("nyx-containers: {message}"),
    }
}

pub const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Ctrl", "F"], "Search"),
    (&["Esc"], "Clear the search"),
    (&["Ctrl", "N"], "New workload"),
    (&["Ctrl", "Tab"], "Next tab"),
    (&["Ctrl", "Shift", "Tab"], "Previous tab"),
    (&["Ctrl", "1…9"], "Go to a tab"),
    (&["Ctrl", "W"], "Close the tab"),
    (&["Ctrl", "Alt"], "Release the keyboard and mouse from a VM display"),
    (&["Ctrl", "R"], "Refresh"),
    (&["Ctrl", "B"], "Collapse or expand the sidebar"),
    (&["Ctrl", ","], "Settings"),
    (&["F1"], "Show these shortcuts (or ?)"),
    (&["Ctrl", "Q"], "Quit"),
];

pub fn show_shortcuts() {
    let (dialog, card) = widgets::dialog("Keyboard shortcuts", 520);
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    for (keys, what) in SHORTCUTS {
        list.append(&widgets::row(what, "", Some(widgets::key_caps(keys).upcast_ref())));
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .propagate_natural_height(true)
        .max_content_height(560)
        .child(&list)
        .build();
    card.append(&scroll);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let d = dialog.clone();
    close.connect_clicked(move |_| d.close());
    card.append(&close);
    dialog.present();
}
