//! What you can do to a workload, shared by the cards and the tabs: open its web UI,
//! the Power button and its menu, the ⋯ menu, snapshots and delete.

use crate::models::{Action, Engine, HostId, Kind, PortMap, RunState, SnapAction, Stack, Workload};
use crate::tabs::Target;
use crate::{prefs, widgets, window, worker};
use gtk::prelude::*;
use nyx_ui::menu;

/// Ports that serve a web page when nothing says otherwise.
const NOT_WEB: &[u16] = &[22, 25, 53, 110, 143, 389, 445, 1883, 3306, 3389, 5432, 5900, 6379, 11211, 27017];

pub fn port_url(host: HostId, port: &PortMap) -> String {
    let scheme = if matches!(port.guest, 443 | 8443 | 9443) || matches!(port.host, 443 | 8443 | 9443) { "https" } else { "http" };
    format!("{scheme}://{}:{}", window::host_address(host), port.host)
}

/// The workload's web UI: its label, else its first published web port.
pub fn web_url(w: &Workload) -> Option<String> {
    if let Some(url) = &w.webui {
        return Some(url.clone());
    }
    w.ports.iter().find(|p| p.proto == "tcp" && !NOT_WEB.contains(&p.guest)).map(|p| port_url(w.host, p))
}

pub fn is_web_port(p: &PortMap) -> bool {
    p.proto == "tcp" && !NOT_WEB.contains(&p.guest)
}

pub fn open_web(w: &Workload) {
    match web_url(w) {
        Some(url) => window::open_uri(&url),
        None => window::toast(&format!("{} doesn't publish a web port.", w.name)),
    }
}

pub fn open(w: &Workload, view: Option<&str>) {
    window::open_tab(Target::Workload { key: w.key() }, view);
}

pub fn default_view(w: &Workload) -> &'static str {
    match (w.kind, w.state) {
        (Kind::VirtualMachine, RunState::Running) => "display",
        _ => "summary",
    }
}

pub fn run(w: &Workload, action: Action) {
    window::toast(&format!("{} {} on {}…", action.gerund(), w.name, window::host_name(w.host)));
    worker::operate(w.host, w.engine, w.target_id().to_string(), action);
}

/// Power: one click stops a running workload or starts a stopped one; the arrow
/// beside it has the rest.
pub fn power_button(w: &Workload) -> gtk::Box {
    let bx = widgets::hbox(0);
    bx.add_css_class("power-split");
    let running = matches!(w.state, RunState::Running | RunState::Paused);
    let main = widgets::icon_button(
        if running { "system-shutdown-symbolic" } else { "media-playback-start-symbolic" },
        if running { "Shut down" } else { "Start" },
    );
    let arrow = widgets::icon_button("pan-down-symbolic", "More power options");
    arrow.add_css_class("power-arrow");
    let w1 = w.clone();
    main.connect_clicked(move |_| run(&w1, if running { Action::Stop } else { Action::Start }));
    let w2 = w.clone();
    arrow.connect_clicked(move |b| menu::popup_below(b, power_entries(&w2)));
    bx.append(&main);
    bx.append(&arrow);
    bx
}

pub fn power_entries(w: &Workload) -> Vec<menu::Entry> {
    let mut out = Vec::new();
    let item = |label: &str, icon: &str, action: Action, w: &Workload| {
        let w = w.clone();
        menu::item(label, move || run(&w, action)).icon(icon)
    };
    match w.state {
        RunState::Running => {
            out.push(item("Shut down", "system-shutdown-symbolic", Action::Stop, w));
            out.push(item("Restart", "view-refresh-symbolic", Action::Restart, w));
            out.push(item("Pause", "media-playback-pause-symbolic", Action::Pause, w));
            out.push(menu::separator());
            out.push(item("Force off", "process-stop-symbolic", Action::Kill, w).danger());
        }
        RunState::Paused => {
            out.push(item("Resume", "media-playback-start-symbolic", Action::Resume, w));
            out.push(item("Force off", "process-stop-symbolic", Action::Kill, w).danger());
        }
        _ => out.push(item("Start", "media-playback-start-symbolic", Action::Start, w)),
    }
    out
}

pub fn is_pinned(key: &str) -> bool {
    prefs::get().pinned.iter().any(|k| k == key)
}

pub fn toggle_pin(key: &str) {
    let key = key.to_string();
    prefs::update(|p| {
        if let Some(i) = p.pinned.iter().position(|k| *k == key) {
            p.pinned.remove(i);
        } else {
            p.pinned.push(key.clone());
        }
    });
    if let Some(snap) = window::snapshot() {
        crate::sections::cards::refresh(&snap, window::host_filter(), "");
    }
}

pub fn ssh_command(w: &Workload) -> Option<String> {
    let ip = w.ip.as_deref()?;
    (w.engine == Engine::Incus).then(|| format!("ssh {}@{ip}", std::env::var("USER").unwrap_or_else(|_| "root".into())))
}

pub fn more_entries(w: &Workload) -> Vec<menu::Entry> {
    let mut out = Vec::new();
    let w1 = w.clone();
    out.push(menu::item("Open in a tab", move || open(&w1, None)).icon("tab-new-symbolic"));
    if let Some(url) = web_url(w) {
        out.push(menu::item("Open web UI", move || window::open_uri(&url)).icon("web-browser-symbolic"));
    }
    let key = w.key();
    let pinned = is_pinned(&key);
    out.push(menu::item(if pinned { "Unpin" } else { "Pin to the top" }, move || toggle_pin(&key)).icon("view-pin-symbolic"));
    out.push(menu::separator());
    if let Some(ip) = w.ip.clone() {
        out.push(menu::item("Copy address", move || {
            window::copy_text(&ip);
            window::toast("Address copied.");
        }));
    }
    if let Some(cmd) = ssh_command(w) {
        out.push(menu::item("Copy SSH command", move || {
            window::copy_text(&cmd);
            window::toast("SSH command copied.");
        }));
    }
    if w.engine == Engine::Incus {
        let w2 = w.clone();
        out.push(menu::item("Take a snapshot", move || quick_snapshot(&w2)).icon("camera-photo-symbolic"));
        let w3 = w.clone();
        out.push(menu::item("Snapshots…", move || open(&w3, Some("snapshots"))));
    }
    let w4 = w.clone();
    out.push(menu::item("Logs", move || open(&w4, Some("logs"))).icon("utilities-system-monitor-symbolic"));
    out.push(menu::separator());
    let w5 = w.clone();
    out.push(menu::item("Delete…", move || confirm_delete(&w5)).icon("user-trash-symbolic").danger());
    out
}

pub fn more_button(w: &Workload) -> gtk::Button {
    let b = widgets::icon_button("view-more-symbolic", "More (F10)");
    let w = w.clone();
    b.connect_clicked(move |b| menu::popup_below(b, more_entries(&w)));
    b
}

pub fn quick_snapshot(w: &Workload) {
    let stamp = glib_stamp();
    window::toast(&format!("Taking snapshot {stamp} of {}…", w.name));
    worker::snap_op(w.host, w.name.clone(), stamp, SnapAction::Create);
}

fn glib_stamp() -> String {
    gtk::glib::DateTime::now_local().ok().and_then(|d| d.format("snap-%Y%m%d-%H%M").ok()).map(|s| s.to_string()).unwrap_or_else(|| "snap".into())
}

pub fn confirm_delete(w: &Workload) {
    let what = match (w.engine, w.kind) {
        (Engine::Incus, Kind::VirtualMachine) => "virtual machine",
        (Engine::Incus, Kind::Container) => "container",
        _ => "Docker container",
    };
    let body = format!("Delete the {what} {} on {}? Its disk and snapshots go with it. This can't be undone.", w.name, window::host_name(w.host));
    let w = w.clone();
    widgets::confirm(&format!("Delete {}?", w.name), &body, "Delete", move || run(&w, Action::Delete));
}

// ---------- Stacks ----------

pub fn stack_web_url(stack: &Stack, services: &[Workload]) -> Option<String> {
    let _ = stack;
    services.iter().find_map(web_url)
}

pub fn stack_power_button(stack: &Stack) -> gtk::Box {
    let bx = widgets::hbox(0);
    bx.add_css_class("power-split");
    let running = stack.running > 0;
    let main = widgets::icon_button(
        if running { "system-shutdown-symbolic" } else { "media-playback-start-symbolic" },
        if running { "Stop the stack" } else { "Start the stack" },
    );
    let arrow = widgets::icon_button("pan-down-symbolic", "More power options");
    arrow.add_css_class("power-arrow");
    let s1 = stack.clone();
    main.connect_clicked(move |_| crate::sections::stacks::run_stack(&s1, if running { Action::Stop } else { Action::Start }));
    let s2 = stack.clone();
    arrow.connect_clicked(move |b| {
        let mut entries = Vec::new();
        let it = |label: &str, icon: &str, action: Action| {
            let s = s2.clone();
            menu::item(label, move || crate::sections::stacks::run_stack(&s, action)).icon(icon)
        };
        if s2.running < s2.total {
            entries.push(it("Up", "media-playback-start-symbolic", Action::Start));
        }
        if s2.running > 0 {
            entries.push(it("Restart", "view-refresh-symbolic", Action::Restart));
            entries.push(it("Pause", "media-playback-pause-symbolic", Action::Pause));
            entries.push(it("Down", "system-shutdown-symbolic", Action::Stop));
        }
        menu::popup_below(b, entries);
    });
    bx.append(&main);
    bx.append(&arrow);
    bx
}

pub fn open_stack(stack: &Stack, view: Option<&str>) {
    window::open_tab(Target::Stack { host: stack.host, project: stack.name.clone() }, view);
}
