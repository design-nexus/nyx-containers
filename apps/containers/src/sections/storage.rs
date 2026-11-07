use crate::models::{Engine, HostFilter, HostId, Snapshot, VolumeSpec};
use crate::sections::cards::short_bytes;
use crate::sections::rescards::{self, Item, ResPage};
use crate::widgets::{self, Page};
use crate::{prefs, window, worker};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static UI: RefCell<Option<Rc<ResPage>>> = const { RefCell::new(None) };
}

pub fn build(page: &Page) {
    let ui = rescards::build(
        page,
        ("drive-harddisk-symbolic", "No storage", "Volumes keep data apart from the workloads that use it."),
        Some(("list-add-symbolic", "New volume", Rc::new(open_create))),
    );
    UI.with(|u| *u.borrow_mut() = Some(ui));
    gtk::glib::idle_add_local_once(|| worker::host_info(HostId::Local));
    if let Some(snap) = window::snapshot() {
        refresh(&snap, window::host_filter(), "");
    }
}

fn kind_icon(kind: &str) -> (&'static str, &'static str, &'static str) {
    match kind {
        "virtual-machine" => ("computer-symbolic", "graphite", "VM disk"),
        "container" => ("package-x-generic-symbolic", "graphite", "Container disk"),
        "image" => ("media-optical-symbolic", "violet", "Cached image"),
        _ => ("folder-symbolic", "teal", "Volume"),
    }
}

pub fn refresh(snapshot: &Snapshot, host: HostFilter, query: &str) {
    let Some(ui) = UI.with(|u| u.borrow().clone()) else { return };
    let q = query.trim().to_ascii_lowercase();
    let mut items = Vec::new();
    let mut hosts: Vec<HostId> = snapshot.volumes(host).map(|v| v.host).collect();
    hosts.push(HostId::Local);
    hosts.sort();
    hosts.dedup();
    for h in hosts.into_iter().filter(|h| host.matches(*h)) {
        let pools = crate::sections::host::pools(h);
        let mut pool_names: Vec<String> = pools.iter().map(|p| p.name.clone()).collect();
        for v in snapshot.volumes(host).filter(|v| v.host == h && v.engine == Engine::Incus) {
            if let Some(p) = &v.pool
                && !pool_names.contains(p)
            {
                pool_names.push(p.clone());
            }
        }
        for name in pool_names {
            let info = pools.iter().find(|p| p.name == name);
            let group = format!("Pool {name} · {}", window::host_name(h));
            let (frac, sub) = match info.and_then(|p| p.used.zip(p.total)) {
                Some((u, t)) if t > 0 => (Some(u as f64 / t as f64), format!("{} free of {}", short_bytes(t.saturating_sub(u)), short_bytes(t))),
                _ => (None, String::new()),
            };
            let vols: Vec<_> = snapshot
                .volumes(host)
                .filter(|v| v.host == h && v.engine == Engine::Incus && v.pool.as_deref() == Some(name.as_str()))
                .filter(|v| q.is_empty() || v.name.to_ascii_lowercase().contains(&q) || v.kind.contains(&q))
                .collect();
            items.push(Item {
                group: group.clone(),
                engine: Engine::Incus,
                icon: "drive-harddisk-symbolic",
                color: "amber",
                title: name.clone(),
                tags: vec![(info.map(|p| p.driver.clone()).unwrap_or_default(), "")],
                sub: format!("{}{} volumes", if sub.is_empty() { String::new() } else { format!("{sub} · ") }, vols.len()),
                chips: Vec::new(),
                facts: Vec::new(),
                bar: Some(frac.unwrap_or(0.0)),
                actions: Vec::new(),
            });
            let mut vols = vols;
            vols.sort_by_key(|v| (v.kind != "custom", v.kind.clone(), v.name.to_ascii_lowercase()));
            for v in vols {
                let (icon, color, what) = kind_icon(&v.kind);
                let mut tags = Vec::new();
                if v.used_by > 0 && v.kind == "custom" {
                    tags.push((format!("Used by {}", v.used_by), "accent"));
                }
                let mut actions: Vec<gtk::Widget> = Vec::new();
                if v.removable {
                    let (hh, name, pool) = (v.host, v.name.clone(), v.pool.clone());
                    actions.push(rescards::remove_button(move || {
                        window::toast(&format!("Removing {name}…"));
                        worker::remove_volume(hh, Engine::Incus, name.clone(), pool.clone());
                    }));
                }
                let size = v.size_label.parse::<u64>().map(short_bytes).unwrap_or_else(|_| v.size_label.clone());
                let title = if v.kind == "image" { v.name.chars().take(12).collect() } else { v.name.clone() };
                items.push(Item {
                    group: group.clone(),
                    engine: Engine::Incus,
                    icon,
                    color,
                    title,
                    tags,
                    sub: [what.to_string(), if size == "—" { String::new() } else { size }].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                    chips: Vec::new(),
                    facts: Vec::new(),
                    bar: None,
                    actions,
                });
            }
        }
        let docker: Vec<_> = snapshot
            .volumes(host)
            .filter(|v| v.host == h && v.engine == Engine::Docker)
            .filter(|v| q.is_empty() || v.name.to_ascii_lowercase().contains(&q))
            .collect();
        for v in docker {
            let anonymous = v.name.len() == 64 && v.name.chars().all(|c| c.is_ascii_hexdigit());
            let mut tags = Vec::new();
            if anonymous {
                tags.push(("Unnamed".to_string(), ""));
            }
            if v.used_by > 0 {
                tags.push((format!("Used by {}", v.used_by), "accent"));
            }
            let mut actions: Vec<gtk::Widget> = Vec::new();
            if v.removable {
                let (hh, name) = (v.host, v.name.clone());
                actions.push(rescards::remove_button(move || {
                    window::toast(&format!("Removing {name}…"));
                    worker::remove_volume(hh, Engine::Docker, name.clone(), None);
                }));
            }
            let title = if anonymous { v.name.chars().take(12).collect() } else { v.name.clone() };
            items.push(Item {
                group: format!("Docker volumes · {}", window::host_name(h)),
                engine: Engine::Docker,
                icon: "folder-symbolic",
                color: "blue",
                title,
                tags,
                sub: [v.kind.clone(), if v.size_label == "—" { String::new() } else { v.size_label.clone() }].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                chips: Vec::new(),
                facts: Vec::new(),
                bar: None,
                actions,
            });
        }
    }
    ui.paint(items);
}

fn open_create() {
    let (dialog, card) = widgets::dialog("Create volume", 480);
    let host = Rc::new(RefCell::new(HostId::Local));
    let engine = Rc::new(RefCell::new(Engine::Docker));
    card.append(&widgets::segmented_row(
        "Host",
        "Where the volume will live.",
        vec![("local".into(), "Local".into()), ("remote".into(), window::host_name(HostId::Remote))],
        "local",
        {
            let host = host.clone();
            move |id| *host.borrow_mut() = if id == "remote" { HostId::Remote } else { HostId::Local }
        },
    ));
    card.append(&widgets::segmented_row(
        "Engine",
        "Docker named volume, or an Incus custom volume.",
        vec![("docker".into(), "Docker".into()), ("incus".into(), "Incus".into())],
        "docker",
        {
            let engine = engine.clone();
            move |id| *engine.borrow_mut() = if id == "incus" { Engine::Incus } else { Engine::Docker }
        },
    ));
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Name"));
    name.set_hexpand(true);
    card.append(&widgets::row("Name", "", Some(name.upcast_ref())));
    let pool = gtk::Entry::new();
    pool.set_placeholder_text(Some("default"));
    pool.set_hexpand(true);
    card.append(&widgets::row("Pool", "Incus storage pool. Ignored for Docker.", Some(pool.upcast_ref())));
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Not now");
    let ok = gtk::Button::with_label("Create");
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);
    card.append(&buttons);
    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());
    let d = dialog.clone();
    ok.connect_clicked(move |_| {
        let n = name.text().trim().to_string();
        if n.is_empty() {
            window::toast("A volume name is required.");
            return;
        }
        let host = *host.borrow();
        let engine = *engine.borrow();
        if host == HostId::Remote && !prefs::get().remote.configured() {
            window::toast("Configure a remote host first.");
            return;
        }
        let pool_text = pool.text().trim().to_string();
        let pool = if engine == Engine::Incus {
            Some(if pool_text.is_empty() { "default".into() } else { pool_text })
        } else {
            None
        };
        window::toast(&format!("Creating {n} on {}…", window::host_name(host)));
        worker::create_volume(VolumeSpec { host, engine, name: n, pool });
        d.close();
    });
    dialog.present();
}

