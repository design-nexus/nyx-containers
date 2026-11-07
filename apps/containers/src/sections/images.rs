use crate::models::{Engine, HostFilter, HostId, ImageDetail, Snapshot};
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
        ("media-optical-symbolic", "No images", "Pull one, or create a workload and its image comes along."),
        Some(("folder-download-symbolic", "Pull an image", Rc::new(open_pull))),
    );
    UI.with(|u| *u.borrow_mut() = Some(ui));
    if let Some(snap) = window::snapshot() {
        refresh(&snap, window::host_filter(), "");
    }
}

pub fn refresh(snapshot: &Snapshot, host: HostFilter, query: &str) {
    let Some(ui) = UI.with(|u| u.borrow().clone()) else { return };
    let q = query.trim().to_ascii_lowercase();
    let mut images: Vec<_> = snapshot.images(host).filter(|i| q.is_empty() || i.name.to_ascii_lowercase().contains(&q) || i.id.to_ascii_lowercase().contains(&q)).collect();
    images.sort_by_key(|i| (i.engine != Engine::Docker, i.host, i.name.to_ascii_lowercase()));
    let items = images
        .into_iter()
        .map(|img| {
            let users: Vec<String> = snapshot
                .workloads
                .iter()
                .filter(|w| w.host == img.host && w.engine == Engine::Docker && img.engine == Engine::Docker)
                .filter(|w| w.image.as_deref().is_some_and(|i| i == img.name || img.id.ends_with(i.trim_start_matches("sha256:")) || format!("{i}:latest") == img.name))
                .map(|w| w.name.clone())
                .collect();
            let dangling = img.name.contains("<none>") || img.name.chars().all(|c| c.is_ascii_hexdigit());
            let mut tags = Vec::new();
            if img.engine == Engine::Docker {
                if dangling {
                    tags.push(("Dangling".to_string(), "warning"));
                } else if users.is_empty() {
                    tags.push(("Unused".to_string(), ""));
                } else {
                    tags.push((format!("In use by {}", users.len()), "accent"));
                }
            }
            let mut sub = vec![crate::sections::cards::short_bytes(img.size)];
            if let Some(d) = img.created.as_deref().and_then(rescards::date) {
                sub.push(format!("created {d}"));
            }
            let short_id: String = img.id.trim_start_matches("sha256:").chars().take(12).collect();
            sub.push(format!("ID {short_id}"));
            let mut actions: Vec<gtk::Widget> = Vec::new();
            if img.engine == Engine::Docker && !dangling {
                let pull = widgets::icon_button("folder-download-symbolic", "Pull a newer copy");
                let (h, name) = (img.host, img.name.clone());
                pull.connect_clicked(move |_| {
                    window::toast(&format!("Pulling {name}…"));
                    worker::pull_image(h, Engine::Docker, name.clone());
                });
                actions.push(pull.upcast());
            }
            let inspect = widgets::icon_button("dialog-information-symbolic", "Details");
            let (h, e, id) = (img.host, img.engine, img.id.clone());
            inspect.connect_clicked(move |_| worker::inspect_image(h, e, id.clone()));
            actions.push(inspect.upcast());
            let (h, e, id, name) = (img.host, img.engine, img.id.clone(), img.name.clone());
            actions.push(rescards::remove_button(move || {
                window::toast(&format!("Removing {name}…"));
                worker::remove_image(h, e, id.clone());
            }));
            Item {
                group: format!("{} · {}", img.engine.label(), window::host_name(img.host)),
                engine: img.engine,
                icon: if img.engine == Engine::Docker { "application-x-executable-symbolic" } else { "media-optical-symbolic" },
                color: if img.engine == Engine::Docker { "blue" } else { "violet" },
                title: img.name.clone(),
                tags,
                sub: sub.join(" · "),
                chips: users,
                facts: Vec::new(),
                bar: None,
                actions,
            }
        })
        .collect();
    ui.paint(items);
}

pub fn show_detail(detail: ImageDetail) {
    let (dialog, card) = widgets::dialog(&detail.image.name, 520);
    card.append(&kv("Host", &window::host_name(detail.image.host)));
    card.append(&kv("Engine", detail.image.engine.label()));
    card.append(&kv("Size", &crate::models::format_bytes(detail.image.size)));
    if let Some(created) = &detail.image.created {
        card.append(&kv("Created", created));
    }
    for (k, v) in &detail.extra {
        card.append(&kv(k, v));
    }
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let d = dialog.clone();
    close.connect_clicked(move |_| d.close());
    card.append(&close);
    dialog.present();
}

fn kv(title: &str, value: &str) -> gtk::Box {
    widgets::row(title, "", Some(widgets::label(value, "").upcast_ref()))
}

fn open_pull() {
    let (dialog, card) = widgets::dialog("Pull image", 480);
    let host = Rc::new(RefCell::new(HostId::Local));
    let engine = Rc::new(RefCell::new(Engine::Docker));
    card.append(&widgets::segmented_row(
        "Host",
        "Where the image will be stored.",
        vec![("local".into(), "Local".into()), ("remote".into(), window::host_name(HostId::Remote))],
        "local",
        {
            let host = host.clone();
            move |id| *host.borrow_mut() = if id == "remote" { HostId::Remote } else { HostId::Local }
        },
    ));
    card.append(&widgets::segmented_row(
        "Engine",
        "Docker Hub tags, or Incus aliases from images.linuxcontainers.org.",
        vec![("docker".into(), "Docker".into()), ("incus".into(), "Incus".into())],
        "docker",
        {
            let engine = engine.clone();
            move |id| *engine.borrow_mut() = if id == "incus" { Engine::Incus } else { Engine::Docker }
        },
    ));
    let reference = gtk::Entry::new();
    reference.set_placeholder_text(Some("nginx:latest"));
    reference.set_hexpand(true);
    card.append(&widgets::row("Image", "", Some(reference.upcast_ref())));
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Not now");
    let ok = gtk::Button::with_label("Pull");
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);
    card.append(&buttons);
    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());
    let d = dialog.clone();
    ok.connect_clicked(move |_| {
        let r = reference.text().trim().to_string();
        if r.is_empty() {
            window::toast("An image name is required.");
            return;
        }
        let host = *host.borrow();
        let engine = *engine.borrow();
        if host == HostId::Remote && !prefs::get().remote.configured() {
            window::toast("Configure a remote host first.");
            return;
        }
        window::toast(&format!("Pulling {r} on {}…", window::host_name(host)));
        worker::pull_image(host, engine, r);
        d.close();
    });
    dialog.present();
}

