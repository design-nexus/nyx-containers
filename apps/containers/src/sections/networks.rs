use crate::models::{Engine, HostFilter, HostId, NetSpec, Snapshot};
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
        ("network-wired-symbolic", "No networks", "Create a bridge to put workloads on a network of their own."),
        Some(("list-add-symbolic", "New network", Rc::new(open_create))),
    );
    UI.with(|u| *u.borrow_mut() = Some(ui));
    if let Some(snap) = window::snapshot() {
        refresh(&snap, window::host_filter(), "");
    }
}

pub fn refresh(snapshot: &Snapshot, host: HostFilter, query: &str) {
    let Some(ui) = UI.with(|u| u.borrow().clone()) else { return };
    let q = query.trim().to_ascii_lowercase();
    let mut nets: Vec<_> = snapshot
        .networks(host)
        .filter(|n| q.is_empty() || n.name.to_ascii_lowercase().contains(&q) || n.driver.to_ascii_lowercase().contains(&q) || n.members.iter().any(|m| m.to_ascii_lowercase().contains(&q)))
        .collect();
    // Networks in use first, then by name.
    nets.sort_by_key(|n| (n.engine != Engine::Incus, n.host, n.members.is_empty(), n.name.to_ascii_lowercase()));
    let items = nets
        .into_iter()
        .map(|n| {
            let mut tags = vec![(n.driver.clone(), "")];
            if !n.removable {
                tags.push((if n.engine == Engine::Docker { "Built in".into() } else { "Unmanaged".into() }, ""));
            }
            let mut sub = Vec::new();
            if let Some(s) = &n.subnet {
                sub.push(s.clone());
            }
            if let Some(g) = &n.gateway {
                sub.push(format!("gateway {g}"));
            }
            if sub.is_empty() {
                sub.push("No IPv4 range".into());
            }
            let attached = if n.members.is_empty() { "Nothing attached".to_string() } else { format!("{} attached", n.members.len()) };
            sub.push(attached);
            let mut facts = Vec::new();
            if let Some(v6) = &n.ipv6 {
                facts.push(("IPv6".to_string(), v6.clone()));
            }
            let mut actions: Vec<gtk::Widget> = Vec::new();
            if n.removable {
                let (h, e, name) = (n.host, n.engine, n.name.clone());
                actions.push(rescards::remove_button(move || {
                    window::toast(&format!("Removing {name}…"));
                    worker::remove_network(h, e, name.clone());
                }));
            }
            Item {
                group: format!("{} · {}", n.engine.label(), window::host_name(n.host)),
                engine: n.engine,
                icon: if n.driver == "physical" || n.driver == "macvlan" { "network-wired-symbolic" } else { "network-workgroup-symbolic" },
                color: if n.engine == Engine::Docker { "blue" } else { "green" },
                title: n.name.clone(),
                tags,
                sub: sub.join(" · "),
                chips: n.members.clone(),
                facts,
                bar: None,
                actions,
            }
        })
        .collect();
    ui.paint(items);
}

fn open_create() {
    let (dialog, card) = widgets::dialog("Create network", 480);
    let host = Rc::new(RefCell::new(HostId::Local));
    let engine = Rc::new(RefCell::new(Engine::Docker));
    card.append(&widgets::segmented_row(
        "Host",
        "Where the network will live.",
        vec![("local".into(), "Local".into()), ("remote".into(), window::host_name(HostId::Remote))],
        "local",
        {
            let host = host.clone();
            move |id| *host.borrow_mut() = if id == "remote" { HostId::Remote } else { HostId::Local }
        },
    ));
    card.append(&widgets::segmented_row(
        "Engine",
        "Docker or Incus bridge.",
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
    let driver = gtk::Entry::new();
    driver.set_placeholder_text(Some("bridge"));
    driver.set_hexpand(true);
    card.append(&widgets::row("Driver", "Docker driver or Incus type. Defaults to bridge.", Some(driver.upcast_ref())));
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
            window::toast("A network name is required.");
            return;
        }
        let host = *host.borrow();
        let engine = *engine.borrow();
        if host == HostId::Remote && !prefs::get().remote.configured() {
            window::toast("Configure a remote host first.");
            return;
        }
        let mut driver = driver.text().trim().to_string();
        if driver.is_empty() {
            driver = "bridge".into();
        }
        window::toast(&format!("Creating {n} on {}…", window::host_name(host)));
        worker::create_network(NetSpec { host, engine, name: n, driver });
        d.close();
    });
    dialog.present();
}

