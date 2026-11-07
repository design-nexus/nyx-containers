//! A tab for one workload or Compose stack: a view switch on the left (Summary,
//! Display, Terminal, Logs, Snapshots…), the workload's main actions on the right,
//! and the views below. Views are built the first time they're shown; a tab's
//! terminals and log streams stop when it closes.

pub mod display;
pub mod hardware;
pub mod inspect;

use crate::history;
use crate::models::{Engine, HostId, InstanceSnap, Kind, RunState, SnapAction, Snapshot, Stack, TermKind, Workload, WorkloadDetail};
use crate::sections::{actions, cards::short_bytes, session};
use crate::{widgets, window, worker};
use gtk::prelude::*;
use vte4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Workload { key: String },
    Stack { host: HostId, project: String },
}

impl Target {
    pub fn key(&self) -> String {
        match self {
            Target::Workload { key } => key.clone(),
            Target::Stack { host, project } => format!("stack|{}|{project}", host.label()),
        }
    }
}

pub struct Info {
    pub title: String,
    pub icon: &'static str,
    pub tooltip: String,
}

fn find<'a>(snap: &'a Snapshot, key: &str) -> Option<&'a Workload> {
    snap.workloads.iter().find(|w| w.key() == key)
}

fn find_stack(snap: &Snapshot, host: HostId, project: &str) -> Option<(Stack, Vec<Workload>)> {
    let stack = snap.stacks(crate::models::HostFilter::All).into_iter().find(|s| s.host == host && s.name == project)?;
    let services = snap
        .workloads
        .iter()
        .filter(|w| w.engine == Engine::Docker && w.host == host && w.project.as_deref() == Some(project))
        .cloned()
        .collect();
    Some((stack, services))
}

pub fn info(target: &Target, snap: &Snapshot) -> Info {
    match target {
        Target::Workload { key } => {
            let name = key.rsplit('|').next().unwrap_or(key).to_string();
            match find(snap, key) {
                Some(w) => Info {
                    title: w.name.clone(),
                    icon: match (w.engine, w.kind) {
                        (Engine::Incus, Kind::VirtualMachine) => "computer-symbolic",
                        (Engine::Incus, _) => "package-x-generic-symbolic",
                        _ => "application-x-executable-symbolic",
                    },
                    tooltip: format!("{} · {} · {}", w.name, w.state.label(), window::host_name(w.host)),
                },
                None => Info { title: name.chars().take(24).collect(), icon: "dialog-question-symbolic", tooltip: "Gone".into() },
            }
        }
        Target::Stack { host, project } => {
            Info { title: project.clone(), icon: "view-list-symbolic", tooltip: format!("Stack {project} · {}", window::host_name(*host)) }
        }
    }
}

struct SummaryUi {
    status: gtk::Label,
    facts: gtk::Box,
    cpu: gtk::Label,
    mem: gtk::Label,
    detail: gtk::Box,
}

struct TabUi {
    target: Target,
    root: gtk::Box,
    nav: RefCell<Vec<(String, gtk::ToggleButton)>>,
    views: gtk::Stack,
    built: RefCell<std::collections::HashSet<String>>,
    actions: gtk::Box,
    sig: RefCell<String>,
    workload: RefCell<Option<Workload>>,
    stack: RefCell<Option<(Stack, Vec<Workload>)>>,
    seqs: RefCell<Vec<u64>>,
    summary: RefCell<Option<SummaryUi>>,
    snaps: RefCell<Option<gtk::Box>>,
    display: RefCell<Option<Rc<display::Display>>>,
    editor: RefCell<Option<Rc<hardware::Editor>>>,
    seg: gtk::Box,
    pick: gtk::Button,
    pick_label: gtk::Label,
}

fn apply_narrow(ui: &TabUi, narrow: bool) {
    ui.seg.set_visible(!narrow);
    ui.pick.set_visible(narrow);
}

pub fn set_narrow(narrow: bool) {
    for ui in TABS.with(|t| t.borrow().values().cloned().collect::<Vec<_>>()) {
        apply_narrow(&ui, narrow);
    }
    hardware::set_narrow(narrow);
}

thread_local! {
    static TABS: RefCell<HashMap<String, Rc<TabUi>>> = RefCell::new(HashMap::new());
}

fn views_for(target: &Target, snap: &Snapshot) -> Vec<(&'static str, &'static str)> {
    match target {
        Target::Stack { .. } => vec![("services", "Services"), ("compose", "Compose"), ("logs", "Logs")],
        Target::Workload { key } => match find(snap, key).map(|w| (w.engine, w.kind)) {
            Some((Engine::Incus, Kind::VirtualMachine)) => vec![
                ("summary", "Summary"),
                ("display", "Display"),
                ("terminal", "Terminal"),
                ("logs", "Logs"),
                ("snapshots", "Snapshots"),
                ("hardware", "Hardware"),
                ("options", "Options"),
            ],
            Some((Engine::Incus, _)) => vec![
                ("summary", "Summary"),
                ("terminal", "Terminal"),
                ("logs", "Logs"),
                ("snapshots", "Snapshots"),
                ("hardware", "Hardware"),
                ("options", "Options"),
            ],
            _ => vec![("summary", "Summary"), ("logs", "Logs"), ("terminal", "Terminal"), ("inspect", "Inspect")],
        },
    }
}

pub fn build(target: &Target, snap: &Snapshot) -> gtk::Widget {
    let root = widgets::vbox(0);
    root.add_css_class("workload-tab");
    let bar = widgets::hbox(8);
    bar.add_css_class("tab-toolbar");
    let seg = widgets::hbox(0);
    seg.add_css_class("segmented");
    seg.add_css_class("tab-views");
    seg.set_valign(gtk::Align::Center);
    bar.append(&seg);
    // In a narrow window the views are a menu instead of a row of segments.
    let pick = gtk::Button::new();
    pick.add_css_class("tab-view-pick");
    let pick_box = widgets::hbox(6);
    let pick_label = widgets::label("", "");
    pick_box.append(&pick_label);
    pick_box.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
    pick.set_child(Some(&pick_box));
    pick.set_valign(gtk::Align::Center);
    pick.set_visible(false);
    bar.append(&pick);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    let acts = widgets::hbox(2);
    acts.set_valign(gtk::Align::Center);
    bar.append(&acts);
    root.append(&bar);
    let views = gtk::Stack::new();
    views.set_hhomogeneous(false);
    views.set_vexpand(true);
    views.set_hexpand(true);
    views.set_transition_type(gtk::StackTransitionType::Crossfade);
    views.set_transition_duration(if crate::prefs::get().reduce_motion { 0 } else { 120 });
    root.append(&views);

    let ui = Rc::new(TabUi {
        target: target.clone(),
        root: root.clone(),
        nav: RefCell::new(Vec::new()),
        views,
        built: RefCell::new(Default::default()),
        actions: acts,
        sig: RefCell::new(String::new()),
        workload: RefCell::new(None),
        stack: RefCell::new(None),
        seqs: RefCell::new(Vec::new()),
        summary: RefCell::new(None),
        snaps: RefCell::new(None),
        display: RefCell::new(None),
        editor: RefCell::new(None),
        seg: seg.clone(),
        pick: pick.clone(),
        pick_label,
    });
    let w = Rc::downgrade(&ui);
    pick.connect_clicked(move |b| {
        let Some(ui) = w.upgrade() else { return };
        let entries = ui
            .nav
            .borrow()
            .iter()
            .map(|(id, button)| {
                let (button, title) = (button.clone(), button.label().map(|l| l.to_string()).unwrap_or_else(|| id.clone()));
                nyx_ui::menu::check(&title, button.is_active(), move |_| button.set_active(true))
            })
            .collect();
        nyx_ui::menu::popup_below(b, entries);
    });
    let mut first: Option<gtk::ToggleButton> = None;
    let mut nav = Vec::new();
    for (id, title) in views_for(target, snap) {
        let b = gtk::ToggleButton::with_label(title);
        b.add_css_class("segment");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        let (w, id_s) = (Rc::downgrade(&ui), id.to_string());
        b.connect_toggled(move |b| {
            if b.is_active()
                && let Some(ui) = w.upgrade()
            {
                show(&ui, &id_s);
            }
        });
        seg.append(&b);
        nav.push((id.to_string(), b));
    }
    *ui.nav.borrow_mut() = nav;
    TABS.with(|t| t.borrow_mut().insert(target.key(), ui.clone()));
    apply_narrow(&ui, window::is_narrow());
    update(&ui, snap);
    if let Some((id, b)) = ui.nav.borrow().first() {
        let _ = id;
        b.set_active(true);
    }
    root.upcast()
}

pub fn show_view(key: &str, view: &str) {
    let Some(ui) = TABS.with(|t| t.borrow().get(key).cloned()) else { return };
    let button = ui.nav.borrow().iter().find(|(id, _)| id == view).map(|(_, b)| b.clone());
    match button {
        Some(b) if b.is_active() => show(&ui, view),
        Some(b) => b.set_active(true),
        None => {}
    }
}

pub fn closed(key: &str) {
    if let Some(ui) = TABS.with(|t| t.borrow_mut().remove(key)) {
        for seq in ui.seqs.borrow().iter() {
            session::stop(*seq);
        }
        if let Some(d) = ui.display.borrow().as_ref() {
            d.close();
        }
    }
}

pub fn set_toolbars_visible(visible: bool) {
    for ui in TABS.with(|t| t.borrow().values().cloned().collect::<Vec<_>>()) {
        if let Some(bar) = ui.root.first_child() {
            bar.set_visible(visible);
        }
    }
}

/// A terminal or a captured VM display has the keyboard.
pub fn focus_grabbed(window: &gtk::ApplicationWindow) -> bool {
    gtk::prelude::GtkWindowExt::focus(window).is_some_and(|f| {
        f.has_css_class("grabs-keys") || f.ancestor(vte4::Terminal::static_type()).is_some()
    })
}

pub fn refresh(snap: &Snapshot) {
    for ui in TABS.with(|t| t.borrow().values().cloned().collect::<Vec<_>>()) {
        update(&ui, snap);
    }
}

pub fn apply_live() {
    for ui in TABS.with(|t| t.borrow().values().cloned().collect::<Vec<_>>()) {
        paint_summary_live(&ui);
    }
}

pub fn apply_detail(detail: WorkloadDetail) {
    let key = crate::models::workload_key(detail.host, detail.engine, &detail.name);
    let ui = TABS.with(|t| {
        let t = t.borrow();
        t.get(&key).cloned().or_else(|| t.values().find(|u| u.workload.borrow().as_ref().is_some_and(|w| w.name == detail.name && w.host == detail.host)).cloned())
    });
    let Some(ui) = ui else { return };
    let summary = ui.summary.borrow();
    let Some(s) = summary.as_ref() else { return };
    clear(&s.detail);
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(image) = &detail.image {
        rows.push(("Image".into(), image.clone()));
    }
    if let Some(created) = &detail.created {
        rows.push(("Created".into(), pretty_time(created)));
    }
    if let Some(cmd) = &detail.command {
        rows.push(("Command".into(), cmd.clone()));
    }
    rows.extend(detail.extra.iter().cloned());
    for (k, v) in rows {
        s.detail.append(&kv(&k, &v));
    }
    if detail.engine == Engine::Incus {
        drop(summary);
        apply_snaps(detail.host, &detail.name, &detail.snapshots);
    }
}

pub fn apply_snaps(host: HostId, instance: &str, snaps: &[InstanceSnap]) {
    for ui in TABS.with(|t| t.borrow().values().cloned().collect::<Vec<_>>()) {
        let matches = ui.workload.borrow().as_ref().is_some_and(|w| w.host == host && w.name == instance && w.engine == Engine::Incus);
        if matches && let Some(list) = ui.snaps.borrow().as_ref() {
            fill_snaps(list, host, instance, snaps);
        }
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

fn pretty_time(s: &str) -> String {
    match crate::models::parse_rfc3339(s).and_then(|t| gtk::glib::DateTime::from_unix_local(t).ok()) {
        Some(d) => d.format("%b %-d, %Y at %H:%M").map(|s| s.to_string()).unwrap_or_else(|_| s.to_string()),
        None => s.to_string(),
    }
}

fn kv(k: &str, v: &str) -> gtk::Box {
    let row = widgets::hbox(10);
    row.add_css_class("tab-kv");
    let key = widgets::label(k, "tab-kv-key");
    key.set_xalign(0.0);
    key.set_size_request(110, -1);
    key.set_valign(gtk::Align::Start);
    let val = widgets::label(v, "tab-kv-value");
    val.set_xalign(0.0);
    val.set_hexpand(true);
    val.set_wrap(true);
    val.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    val.set_selectable(true);
    row.append(&key);
    row.append(&val);
    row
}

fn update(ui: &Rc<TabUi>, snap: &Snapshot) {
    match &ui.target {
        Target::Workload { key } => {
            let w = find(snap, key).cloned();
            let sig = w.as_ref().map(|w| format!("{:?}|{}", w.state, actions::web_url(w).is_some())).unwrap_or_default();
            *ui.workload.borrow_mut() = w.clone();
            if *ui.sig.borrow() != sig {
                *ui.sig.borrow_mut() = sig;
                clear(&ui.actions);
                if let Some(w) = &w {
                    if actions::web_url(w).is_some() {
                        let web = widgets::icon_button("web-browser-symbolic", "Open the web UI");
                        let w1 = w.clone();
                        web.connect_clicked(move |_| actions::open_web(&w1));
                        ui.actions.append(&web);
                    }
                    if w.engine == Engine::Incus {
                        let snapb = widgets::icon_button("camera-photo-symbolic", "Take a snapshot");
                        let w2 = w.clone();
                        snapb.connect_clicked(move |_| actions::quick_snapshot(&w2));
                        ui.actions.append(&snapb);
                    }
                    ui.actions.append(&actions::power_button(w));
                    ui.actions.append(&actions::more_button(w));
                }
            }
            paint_summary(ui);
        }
        Target::Stack { host, project } => {
            let found = find_stack(snap, *host, project);
            let sig = found.as_ref().map(|(s, _)| format!("{}|{}", s.running, s.total)).unwrap_or_default();
            *ui.stack.borrow_mut() = found.clone();
            if *ui.sig.borrow() != sig {
                *ui.sig.borrow_mut() = sig;
                clear(&ui.actions);
                if let Some((stack, services)) = &found {
                    if let Some(url) = actions::stack_web_url(stack, services) {
                        let web = widgets::icon_button("web-browser-symbolic", "Open the web UI");
                        web.connect_clicked(move |_| window::open_uri(&url));
                        ui.actions.append(&web);
                    }
                    ui.actions.append(&actions::stack_power_button(stack));
                }
                if ui.built.borrow().contains("services") {
                    ui.built.borrow_mut().remove("services");
                    if let Some(child) = ui.views.child_by_name("services") {
                        ui.views.remove(&child);
                    }
                    let current = ui.nav.borrow().iter().find(|(_, b)| b.is_active()).map(|(id, _)| id.clone());
                    if current.as_deref() == Some("services") {
                        show(ui, "services");
                    }
                }
            }
        }
    }
}

fn show(ui: &Rc<TabUi>, view: &str) {
    if !ui.built.borrow().contains(view) {
        let widget = build_view(ui, view);
        ui.views.add_named(&widget, Some(view));
        ui.built.borrow_mut().insert(view.to_string());
    }
    ui.views.set_visible_child_name(view);
    if let Some((_, b)) = ui.nav.borrow().iter().find(|(id, _)| id == view) {
        ui.pick_label.set_text(&b.label().map(|l| l.to_string()).unwrap_or_default());
    }
}

fn build_view(ui: &Rc<TabUi>, view: &str) -> gtk::Widget {
    let w = ui.workload.borrow().clone();
    match (view, &ui.target) {
        ("summary", _) => build_summary(ui),
        ("logs", Target::Stack { host, project }) => {
            let names = ui.stack.borrow().as_ref().map(|(_, s)| s.iter().map(|w| w.target_id().to_string()).collect()).unwrap_or_default();
            let (root, seq) = session::logs_view(*host, Engine::Docker, names, project);
            ui.seqs.borrow_mut().push(seq);
            padded(&root)
        }
        ("logs", _) => match w {
            Some(w) => {
                let (root, seq) = session::logs_view(w.host, w.engine, vec![w.target_id().to_string()], &w.name);
                ui.seqs.borrow_mut().push(seq);
                padded(&root)
            }
            None => gone(),
        },
        ("terminal", _) => match w {
            Some(w) if w.state == RunState::Running => {
                let kind = TermKind::Exec;
                let (root, term, seq) = session::term_view(w.host, w.engine, w.target_id(), kind);
                ui.seqs.borrow_mut().push(seq);
                let wrap = widgets::vbox(0);
                let bar = widgets::hbox(6);
                bar.add_css_class("tab-subbar");
                bar.append(&widgets::label(
                    &format!("{} on {}", if w.engine == Engine::Docker { "Shell in the container" } else { "Shell" }, window::host_name(w.host)),
                    "dim",
                ));
                let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                spacer.set_hexpand(true);
                bar.append(&spacer);
                bar.append(&widgets::label("Paste: Ctrl+Shift+V, Shift+Insert or middle-click", "tab-hint"));
                if w.engine == Engine::Incus {
                    let console = widgets::icon_button("utilities-terminal-symbolic", "Open the boot console in a new view");
                    let w2 = w.clone();
                    let ui_w = Rc::downgrade(ui);
                    console.connect_clicked(move |_| {
                        if let Some(ui) = ui_w.upgrade() {
                            open_console(&ui, &w2);
                        }
                    });
                    bar.append(&console);
                }
                let ext = widgets::icon_button("window-new-symbolic", "Open in your terminal app");
                let w3 = w.clone();
                ext.connect_clicked(move |_| open_external_terminal(&w3));
                bar.append(&ext);
                let t2 = term.clone();
                let copy = widgets::icon_button("edit-copy-symbolic", "Copy (Ctrl+Shift+C)");
                copy.connect_clicked(move |_| t2.copy_clipboard_format(vte4::Format::Text));
                bar.append(&copy);
                let t3 = term.clone();
                let paste = widgets::icon_button("edit-paste-symbolic", "Paste (Ctrl+Shift+V)");
                paste.connect_clicked(move |_| t3.paste_clipboard());
                bar.append(&paste);
                wrap.append(&bar);
                wrap.append(&root);
                let t = term.clone();
                gtk::glib::idle_add_local_once(move || {
                    t.grab_focus();
                });
                padded(&wrap)
            }
            Some(w) => not_running(&w, "The terminal opens once it's running."),
            None => gone(),
        },
        ("display", _) => match w {
            Some(w) => {
                let d = display::Display::new(&w);
                let widget = d.root.clone().upcast::<gtk::Widget>();
                *ui.display.borrow_mut() = Some(d);
                widget
            }
            None => gone(),
        },
        ("hardware", _) => match w {
            Some(w) => {
                let (widget, editor) = hardware::build_hardware(&w);
                *ui.editor.borrow_mut() = Some(editor);
                widget
            }
            None => gone(),
        },
        ("options", _) => match w {
            Some(w) => {
                let (widget, editor) = hardware::build_options(&w);
                *ui.editor.borrow_mut() = Some(editor);
                widget
            }
            None => gone(),
        },
        ("inspect", _) => match w {
            Some(w) => inspect::build(&w),
            None => gone(),
        },
        ("snapshots", _) => match w {
            Some(w) => build_snapshots(ui, &w),
            None => gone(),
        },
        ("services", Target::Stack { .. }) => build_services(ui),
        ("compose", Target::Stack { .. }) => build_compose(ui),
        _ => gone(),
    }
}

fn padded(child: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let b = widgets::vbox(0);
    b.add_css_class("tab-pad");
    b.append(child);
    child.as_ref().set_vexpand(true);
    b.upcast()
}

fn gone() -> gtk::Widget {
    widgets::empty_state("dialog-question-symbolic", "It's gone", "This workload isn't on the host any more.", None).upcast()
}

fn not_running(w: &Workload, desc: &str) -> gtk::Widget {
    let w2 = w.clone();
    widgets::empty_state(
        "media-playback-start-symbolic",
        &format!("{} isn't running", w.name),
        desc,
        Some(("Start", Box::new(move || actions::run(&w2, crate::models::Action::Start)))),
    )
    .upcast()
}

fn open_console(ui: &Rc<TabUi>, w: &Workload) {
    let (root, _term, seq) = session::term_view(w.host, w.engine, w.target_id(), TermKind::Console);
    ui.seqs.borrow_mut().push(seq);
    let name = format!("console-{seq}");
    ui.views.add_named(&padded(&root), Some(&name));
    ui.views.set_visible_child_name(&name);
}

fn open_external_terminal(w: &Workload) {
    let cmd: Vec<String> = match (w.engine, w.host) {
        (Engine::Incus, HostId::Local) => vec!["incus".into(), "exec".into(), w.name.clone(), "--".into(), "sh".into(), "-c".into(), "exec bash || exec sh".into()],
        (Engine::Docker, HostId::Local) => vec!["docker".into(), "exec".into(), "-it".into(), w.target_id().to_string(), "sh".into(), "-c".into(), "exec bash || exec sh".into()],
        _ => {
            window::toast("Opening a remote shell in another terminal isn't set up yet; use the Terminal view.");
            return;
        }
    };
    let launcher = ["xdg-terminal-exec", "alacritty", "kitty", "foot", "ghostty"].into_iter().find(|t| {
        std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(t).exists()))
    });
    let Some(term) = launcher else {
        window::toast("No terminal app found.");
        return;
    };
    let mut c = std::process::Command::new(term);
    if term != "xdg-terminal-exec" {
        c.arg("-e");
    }
    c.args(&cmd);
    if let Err(e) = c.spawn() {
        window::toast(&format!("Couldn't open {term}: {e}"));
    }
}

// ---------- Summary ----------

fn card(title: &str, icon: &str) -> (gtk::Box, gtk::Box) {
    let c = widgets::vbox(0);
    c.add_css_class("tab-card");
    let head = widgets::hbox(8);
    head.add_css_class("tab-card-head");
    let i = gtk::Image::from_icon_name(icon);
    i.add_css_class("accent");
    head.append(&i);
    head.append(&widgets::label(title, "tab-card-title"));
    c.append(&head);
    let body = widgets::vbox(2);
    body.add_css_class("tab-card-body");
    c.append(&body);
    (c, body)
}

fn build_summary(ui: &Rc<TabUi>) -> gtk::Widget {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(2);
    flow.set_min_children_per_line(1);
    flow.set_homogeneous(true);
    flow.set_row_spacing(12);
    flow.set_column_spacing(12);
    flow.set_valign(gtk::Align::Start);
    flow.add_css_class("tab-summary");
    let (status_card, status_body) = card("Status", "emblem-ok-symbolic");
    let status = widgets::label("", "tab-status");
    status.set_xalign(0.0);
    status_body.append(&status);
    let facts = widgets::vbox(2);
    status_body.append(&facts);
    let (res_card, res_body) = card("Live use", "utilities-system-monitor-symbolic");
    let cpu = widgets::label("—", "tab-big");
    cpu.set_xalign(0.0);
    let mem = widgets::label("—", "tab-big");
    mem.set_xalign(0.0);
    res_body.append(&kv_widget("CPU", &cpu));
    res_body.append(&kv_widget("Memory", &mem));
    let (detail_card, detail) = card("Details", "dialog-information-symbolic");
    detail.append(&widgets::label("Loading…", "dim"));
    flow.append(&status_card);
    flow.append(&res_card);
    flow.append(&detail_card);
    *ui.summary.borrow_mut() = Some(SummaryUi { status, facts, cpu, mem, detail });
    paint_summary(ui);
    paint_summary_live(ui);
    if let Some(w) = ui.workload.borrow().as_ref() {
        worker::inspect(w.host, w.engine, w.target_id().to_string());
    }
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&flow).build();
    scroll.add_css_class("tab-scroll");
    scroll.upcast()
}

fn kv_widget(k: &str, v: &gtk::Label) -> gtk::Box {
    let row = widgets::hbox(10);
    row.add_css_class("tab-kv");
    let key = widgets::label(k, "tab-kv-key");
    key.set_xalign(0.0);
    key.set_size_request(110, -1);
    row.append(&key);
    v.set_hexpand(true);
    row.append(v);
    row
}

fn paint_summary(ui: &Rc<TabUi>) {
    let summary = ui.summary.borrow();
    let Some(s) = summary.as_ref() else { return };
    let Some(w) = ui.workload.borrow().clone() else {
        s.status.set_text("Not found on the host");
        return;
    };
    let now = crate::models::now_secs();
    let uptime = w.started.map(|t| format!(" for {}", crate::models::format_uptime(t, now))).unwrap_or_default();
    s.status.set_text(&format!("{}{uptime}", w.state.label()));
    clear(&s.facts);
    s.facts.append(&kv("Host", &window::host_name(w.host)));
    s.facts.append(&kv("Type", match (w.engine, w.kind) {
        (Engine::Incus, Kind::VirtualMachine) => "Virtual machine (Incus)",
        (Engine::Incus, _) => "Linux container (Incus)",
        _ => "Docker container",
    }));
    if let Some(os) = &w.os {
        s.facts.append(&kv("System", os));
    }
    if let Some(ip) = &w.ip {
        s.facts.append(&kv("Address", ip));
    }
    if !w.ports.is_empty() {
        let ports: Vec<String> = w.ports.iter().map(|p| format!("{} → {} {}", p.host, p.guest, p.proto)).collect();
        s.facts.append(&kv("Ports", &ports.join(", ")));
    }
    if let Some(url) = actions::web_url(&w) {
        s.facts.append(&kv("Web UI", &url));
    }
    if let Some(c) = w.cpus {
        s.facts.append(&kv("CPUs", &format!("{c}")));
    }
    if let Some(m) = w.mem_limit {
        s.facts.append(&kv("Memory limit", &short_bytes(m)));
    }
    if w.engine == Engine::Incus {
        s.facts.append(&kv("Autostart", if w.autostart { "On" } else { "Off" }));
    }
    if let Some(p) = &w.project {
        s.facts.append(&kv("Stack", p));
    }
}

fn paint_summary_live(ui: &Rc<TabUi>) {
    let summary = ui.summary.borrow();
    let Some(s) = summary.as_ref() else { return };
    let Some(w) = ui.workload.borrow().clone() else { return };
    let last = history::get(&w.key()).and_then(|h| h.last);
    s.cpu.set_text(&last.as_ref().and_then(|l| l.cpu).map(|c| format!("{c:.1}%")).unwrap_or_else(|| "—".into()));
    let used = last.as_ref().and_then(|l| l.mem);
    let limit = w.mem_limit.or(last.as_ref().and_then(|l| l.mem_limit));
    s.mem.set_text(&match (used, limit) {
        (Some(u), Some(l)) => format!("{} of {}", short_bytes(u), short_bytes(l)),
        (Some(u), None) => short_bytes(u),
        _ => "—".into(),
    });
}

// ---------- Snapshots ----------

fn build_snapshots(ui: &Rc<TabUi>, w: &Workload) -> gtk::Widget {
    let root = widgets::vbox(12);
    root.add_css_class("tab-pad");
    let bar = widgets::hbox(8);
    let note = widgets::label("Snapshots keep the disk as it is now, so you can roll back. VMs can keep their memory too.", "dim");
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_hexpand(true);
    bar.append(&note);
    let take = gtk::Button::with_label("Take snapshot");
    take.add_css_class("suggested-action");
    let w1 = w.clone();
    take.connect_clicked(move |_| actions::quick_snapshot(&w1));
    bar.append(&take);
    root.append(&bar);
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    list.append(&widgets::label("Loading…", "dim"));
    root.append(&list);
    *ui.snaps.borrow_mut() = Some(list);
    worker::inspect(w.host, w.engine, w.target_id().to_string());
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&root).build();
    scroll.upcast()
}

fn fill_snaps(list: &gtk::Box, host: HostId, instance: &str, snaps: &[InstanceSnap]) {
    clear(list);
    if snaps.is_empty() {
        list.append(&widgets::row("No snapshots yet", "Take one before you change something you might want back.", None));
        return;
    }
    let mut snaps = snaps.to_vec();
    snaps.sort_by(|a, b| b.created.cmp(&a.created));
    for snap in snaps {
        let when = snap.created.as_deref().map(pretty_time).unwrap_or_default();
        let desc = if snap.stateful { format!("{when} · with memory") } else { when };
        let buttons = widgets::hbox(6);
        let (inst, name) = (instance.to_string(), snap.name.clone());
        let restore = widgets::two_click("Restore", "Click again to restore", move || {
            window::toast(&format!("Restoring {inst} to {name}…"));
            worker::snap_op(host, inst.clone(), name.clone(), SnapAction::Restore);
        });
        restore.remove_css_class("destructive-action");
        let (inst, name) = (instance.to_string(), snap.name.clone());
        let delete = widgets::two_click("Delete", "Click again to delete", move || {
            window::toast(&format!("Deleting snapshot {name}…"));
            worker::snap_op(host, inst.clone(), name.clone(), SnapAction::Delete);
        });
        buttons.append(&restore);
        buttons.append(&delete);
        list.append(&widgets::row(&snap.name, &desc, Some(buttons.upcast_ref())));
    }
}

// ---------- Stacks ----------

fn build_services(ui: &Rc<TabUi>) -> gtk::Widget {
    let root = widgets::vbox(12);
    root.add_css_class("tab-pad");
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    let found = ui.stack.borrow().clone();
    match found {
        Some((_stack, services)) => {
            for w in services {
                let buttons = widgets::hbox(2);
                if let Some(url) = actions::web_url(&w) {
                    let web = widgets::icon_button("web-browser-symbolic", &format!("Open {url}"));
                    web.connect_clicked(move |_| window::open_uri(&url));
                    buttons.append(&web);
                }
                let open = widgets::icon_button("tab-new-symbolic", "Open in a tab");
                let w1 = w.clone();
                open.connect_clicked(move |_| actions::open(&w1, None));
                buttons.append(&open);
                buttons.append(&actions::power_button(&w));
                let ports: Vec<String> = w.ports.iter().map(|p| format!(":{}", p.host)).collect();
                let desc = format!(
                    "{} · {}{}",
                    w.image.as_deref().unwrap_or(""),
                    w.status,
                    if ports.is_empty() { String::new() } else { format!(" · {}", ports.join(" ")) }
                );
                list.append(&widgets::row(w.service.as_deref().unwrap_or(&w.name), &desc, Some(buttons.upcast_ref())));
            }
        }
        None => list.append(&widgets::row("This stack isn't running anywhere", "Deploy it from the Compose view.", None)),
    }
    root.append(&list);
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&root).build();
    scroll.upcast()
}

fn build_compose(ui: &Rc<TabUi>) -> gtk::Widget {
    use sourceview5::prelude::*;
    let found = ui.stack.borrow().clone();
    let root = widgets::vbox(0);
    root.add_css_class("tab-pad");
    let Some((stack, _)) = found else { return gone() };
    let bar = widgets::hbox(6);
    bar.add_css_class("tab-subbar");
    let path = crate::sections::stacks::compose_paths(&stack).map(|(_, f)| f);
    bar.append(&widgets::label(path.as_deref().unwrap_or("No compose file known for this stack"), "dim mono-path"));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    let validate = gtk::Button::with_label("Validate");
    let save = gtk::Button::with_label("Save");
    let pull = gtk::Button::with_label("Pull");
    let deploy = gtk::Button::with_label("Deploy");
    deploy.add_css_class("suggested-action");
    for b in [&validate, &pull, &save, &deploy] {
        bar.append(b);
    }
    root.append(&bar);
    let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
    buffer.set_language(sourceview5::LanguageManager::default().language("yaml").as_ref());
    widgets::apply_source_scheme(&buffer);
    let text = crate::sections::stacks::load_yaml(&stack).unwrap_or_else(|| "# The compose file isn't readable from here.\n".into());
    buffer.set_text(&text);
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_auto_indent(true);
    view.set_tab_width(2);
    view.set_insert_spaces_instead_of_tabs(true);
    view.set_top_margin(10);
    view.set_bottom_margin(10);
    view.set_left_margin(8);
    view.add_css_class("compose-editor");
    let scroll = gtk::ScrolledWindow::builder().vexpand(true).hexpand(true).child(&view).build();
    scroll.add_css_class("tab-well");
    root.append(&scroll);
    let text_of = {
        let b = buffer.clone();
        move || b.text(&b.start_iter(), &b.end_iter(), false).to_string()
    };
    let t = text_of.clone();
    validate.connect_clicked(move |_| match crate::services::compose::validate_yaml(&t()) {
        Ok(()) => window::toast("The YAML is valid."),
        Err(e) => window::toast(&e),
    });
    let local = stack.host == HostId::Local;
    let s1 = stack.clone();
    let b1 = buffer.clone();
    let save_fn = move || -> Option<(String, String, String)> {
        let dir = s1.project_dir.clone().unwrap_or_default();
        match crate::sections::stacks::write_compose(&s1.name, &dir, &b1, Some(&s1)) {
            Ok(r) => Some(r),
            Err(e) => {
                window::toast(&e);
                None
            }
        }
    };
    let sf = save_fn.clone();
    save.connect_clicked(move |_| {
        if let Some((_, _, file)) = sf() {
            window::toast(&format!("Saved {file}"));
        }
    });
    let s2 = stack.clone();
    pull.connect_clicked(move |_| {
        if let Some((dir, file)) = crate::sections::stacks::compose_paths(&s2) {
            window::toast(&format!("Pulling images for {}…", s2.name));
            worker::compose_pull(s2.host, s2.name.clone(), dir, file);
        }
    });
    deploy.connect_clicked(move |_| {
        if !local {
            window::toast("Deploy runs Compose on this computer; this stack is on the remote host.");
            return;
        }
        if let Some((project, dir, file)) = save_fn() {
            window::toast(&format!("Deploying {project}…"));
            worker::compose(HostId::Local, project, dir, file, crate::models::Action::Start);
        }
    });
    if !local {
        deploy.set_sensitive(false);
        deploy.set_tooltip_text(Some("Compose deploys run on this computer"));
    }
    root.upcast()
}
