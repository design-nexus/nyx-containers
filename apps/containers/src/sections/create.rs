//! New workload: a VM or Linux container from an image (or a VM from an ISO), a
//! Docker container, or a Compose stack. One dialog; the type switch at the top
//! decides which fields show.

use crate::models::{CreateSpec, Engine, HostId, Kind};
use crate::{prefs, widgets, window, worker};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum What {
    Vm,
    Lxc,
    Docker,
    Stack,
}

/// Images on the public Incus image server: (alias, name, note, colour, letter).
/// Images on the public Incus image server: (alias, name, note, tile icon).
const IMAGES: &[(&str, &str, &str, &str)] = &[
    ("ubuntu/24.04", "Ubuntu 24.04 LTS", "Long-term support", "nyx-os-ubuntu"),
    ("ubuntu/24.04/desktop", "Ubuntu Desktop", "24.04 with GNOME · VMs only", "nyx-os-ubuntu"),
    ("debian/13", "Debian 13", "Stable", "nyx-os-debian"),
    ("fedora/43", "Fedora 43", "Latest", "nyx-os-fedora"),
    ("archlinux", "Arch Linux", "Rolling", "nyx-os-archlinux"),
    ("nixos/26.05", "NixOS 26.05", "Declarative", "nyx-os-nixos"),
    ("opensuse/tumbleweed", "openSUSE Tumbleweed", "Rolling", "nyx-os-opensuse"),
    ("alpine/3.21", "Alpine 3.21", "Tiny", "nyx-os-alpine"),
    ("rockylinux/9", "Rocky Linux 9", "RHEL-compatible", "nyx-os-rockylinux"),
];

const VM_ONLY: &[&str] = &["ubuntu/24.04/desktop"];

struct Form {
    what: RefCell<What>,
    host: RefCell<HostId>,
    image: RefCell<String>,
    iso: RefCell<Option<String>>,
    name: gtk::Entry,
    cpus: gtk::SpinButton,
    memory: gtk::SpinButton,
    disk: gtk::SpinButton,
    start: gtk::Switch,
    share: gtk::Switch,
    docker_image: gtk::Entry,
    docker_name: gtk::Entry,
    ports: gtk::Entry,
    volumes: gtk::Entry,
    env: gtk::TextView,
    restart: gtk::DropDown,
    incus_box: gtk::Box,
    vm_only: Vec<gtk::Widget>,
    docker_box: gtk::Box,
    stack_box: gtk::Box,
    os_button: gtk::Button,
    os_icon: gtk::Image,
    os_title: gtk::Label,
    os_note: gtk::Label,
    iso_button: gtk::Button,
    iso_note: gtk::Label,
}

pub fn open(kind: Option<Kind>) {
    let (card, close) = window::overlay_card("New workload", 720);
    card.add_css_class("create-card");

    let types = widgets::hbox(0);
    types.add_css_class("segmented");
    types.add_css_class("create-types");
    types.set_halign(gtk::Align::Start);
    card.append(&types);
    let body = widgets::vbox(12);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(500)
        .child(&body)
        .build();
    card.append(&scroll);

    let remote = prefs::get().remote;
    let host_row = if remote.configured() {
        let host = Rc::new(RefCell::new(HostId::Local));
        let h = host.clone();
        Some((
            widgets::segmented_row(
                "Host",
                "Where it runs.",
                vec![("local".into(), "This computer".into()), ("remote".into(), remote.display_name())],
                "local",
                move |id| *h.borrow_mut() = if id == "remote" { HostId::Remote } else { HostId::Local },
            ),
            host,
        ))
    } else {
        None
    };

    // ----- Incus: image, resources -----
    let incus_box = widgets::vbox(10);
    let picker = widgets::hbox(8);
    picker.add_css_class("create-images");
    let (os_button, os_icon, os_title, os_note) = os_card("pan-down-symbolic");
    os_button.set_hexpand(true);
    os_button.set_tooltip_text(Some("Pick the system"));
    picker.append(&os_button);
    let (iso_button, iso_icon, iso_title, iso_note) = os_card("document-open-symbolic");
    iso_icon.set_icon_name(Some("nyx-os-iso"));
    iso_title.set_text("From an ISO…");
    iso_note.set_text("Windows or any installer");
    picker.append(&iso_button);
    incus_box.append(&picker);
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("A name, e.g. dev-box"));
    name.set_hexpand(true);
    let user_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) as f64;
    let cpus = gtk::SpinButton::with_range(1.0, user_cores, 1.0);
    cpus.set_value(2.0_f64.max((user_cores / 4.0).floor()).min(4.0));
    let memory = gtk::SpinButton::with_range(0.5, 64.0, 0.5);
    memory.set_digits(1);
    memory.set_value(4.0);
    let disk = gtk::SpinButton::with_range(4.0, 2048.0, 4.0);
    disk.set_value(32.0);
    let start = gtk::Switch::new();
    start.set_active(true);
    let share = gtk::Switch::new();
    let res = widgets::vbox(0);
    res.add_css_class("group-list");
    res.append(&widgets::row("Name", "", Some(name.upcast_ref())));
    res.append(&widgets::row("Cores", "", Some(cpus.upcast_ref())));
    res.append(&widgets::row("Memory (GiB)", "", Some(memory.upcast_ref())));
    res.append(&widgets::row("Disk (GiB)", "", Some(disk.upcast_ref())));
    let home = crate::paths::home().join("Projects");
    let share_row = widgets::row(
        "Share ~/Projects",
        &format!("{} appears inside at the same path.", home.display()),
        Some(share.upcast_ref()),
    );
    share_row.set_visible(home.exists());
    res.append(&share_row);
    res.append(&widgets::row("Start when created", "", Some(start.upcast_ref())));
    incus_box.append(&res);

    // ----- Docker -----
    let docker_box = widgets::vbox(0);
    docker_box.add_css_class("group-list");
    let docker_image = gtk::Entry::new();
    docker_image.set_placeholder_text(Some("nginx:latest, ghcr.io/owner/app:tag"));
    docker_image.set_hexpand(true);
    let ports = gtk::Entry::new();
    ports.set_placeholder_text(Some("8080:80, 8443:443"));
    ports.set_hexpand(true);
    let volumes = gtk::Entry::new();
    volumes.set_placeholder_text(Some("~/app/data:/data"));
    volumes.set_hexpand(true);
    let env = gtk::TextView::new();
    env.set_monospace(true);
    env.set_size_request(-1, 72);
    env.add_css_class("create-env");
    let restart = widgets::dropdown(
        &widgets::opts(&[("no", "Never"), ("unless-stopped", "Unless stopped"), ("always", "Always"), ("on-failure", "When it fails")]),
        "unless-stopped",
    );
    let docker_name = gtk::Entry::new();
    docker_name.set_placeholder_text(Some("From the image when empty"));
    docker_name.set_hexpand(true);
    docker_box.append(&widgets::row("Image", "Pulled first if it isn't here yet.", Some(docker_image.upcast_ref())));
    docker_box.append(&widgets::row("Name", "", Some(docker_name.upcast_ref())));
    docker_box.append(&widgets::row("Ports", "Host port : container port.", Some(ports.upcast_ref())));
    docker_box.append(&widgets::row("Folders", "Host folder : path inside.", Some(volumes.upcast_ref())));
    let env_row = widgets::vbox(6);
    env_row.add_css_class("settings-option");
    env_row.append(&widgets::label("Environment, one KEY=value per line", "dim"));
    let env_scroll = gtk::ScrolledWindow::builder().child(&env).min_content_height(72).build();
    env_scroll.add_css_class("tab-well");
    env_row.append(&env_scroll);
    docker_box.append(&env_row);
    docker_box.append(&widgets::row("Restart", "", Some(restart.upcast_ref())));

    // ----- Stack -----
    let stack_box = widgets::empty_state(
        "view-list-symbolic",
        "A Compose stack",
        "Write or paste a compose.yaml, pick its folder, and deploy it.",
        Some(("Open the editor", Box::new(|| crate::sections::stacks::open_editor(None)))),
    );

    let form = Rc::new(Form {
        what: RefCell::new(What::Vm),
        host: RefCell::new(HostId::Local),
        image: RefCell::new("ubuntu/24.04".into()),
        iso: RefCell::new(None),
        name,
        cpus,
        memory,
        disk,
        start,
        share,
        docker_image,
        docker_name,
        ports,
        volumes,
        env,
        restart,
        incus_box: incus_box.clone(),
        vm_only: Vec::new(),
        docker_box: docker_box.clone(),
        stack_box: stack_box.clone(),
        os_button,
        os_icon,
        os_title,
        os_note,
        iso_button,
        iso_note,
    });
    wire_picker(&form);

    if let Some((row, _)) = &host_row {
        body.append(row);
    }
    body.append(&incus_box);
    body.append(&docker_box);
    body.append(&stack_box);

    let initial = match kind {
        Some(Kind::Container) => What::Lxc,
        _ => What::Vm,
    };
    let mut first: Option<gtk::ToggleButton> = None;
    for (what, label) in [(What::Vm, "Virtual machine"), (What::Lxc, "Linux container"), (What::Docker, "Docker"), (What::Stack, "Compose stack")] {
        let b = gtk::ToggleButton::with_label(label);
        b.add_css_class("segment");
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        let f = form.clone();
        b.connect_toggled(move |b| {
            if b.is_active() {
                *f.what.borrow_mut() = what;
                show_what(&f);
            }
        });
        if what == initial {
            b.set_active(true);
        }
        types.append(&b);
    }
    *form.what.borrow_mut() = initial;
    show_what(&form);

    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Not now");
    let ok = gtk::Button::with_label("Create");
    ok.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&ok);
    card.append(&buttons);
    let c = close.clone();
    cancel.connect_clicked(move |_| c());
    let host_cell = host_row.map(|(_, h)| h);
    ok.connect_clicked(move |_| {
        if let Some(h) = &host_cell {
            *form.host.borrow_mut() = *h.borrow();
        }
        if *form.what.borrow() == What::Stack {
            close();
            crate::sections::stacks::open_editor(None);
            return;
        }
        match spec(&form) {
            Ok(spec) => {
                window::toast(&format!(
                    "Creating {} on {}{}",
                    spec.name,
                    window::host_name(spec.host),
                    if spec.from_server { " · downloading the image can take a few minutes" } else { "…" }
                ));
                worker::create(spec);
                close();
            }
            Err(e) => window::toast(&e),
        }
    });
}

fn show_what(f: &Rc<Form>) {
    let what = *f.what.borrow();
    f.incus_box.set_visible(matches!(what, What::Vm | What::Lxc));
    f.docker_box.set_visible(what == What::Docker);
    f.stack_box.set_visible(what == What::Stack);
    for w in &f.vm_only {
        w.set_visible(what == What::Vm);
    }
    f.iso_button.set_visible(what == What::Vm);
    let on_vm_only = f.iso.borrow().is_some() || VM_ONLY.contains(&f.image.borrow().as_str());
    if what == What::Lxc && on_vm_only {
        f.iso.borrow_mut().take();
        select_os(f, "ubuntu/24.04");
    }
    let (cpu, mem, disk): (f64, f64, f64) = if what == What::Vm { (4.0, 8.0, 64.0) } else { (2.0, 2.0, 16.0) };
    f.cpus.set_value(cpu.min(f.cpus.adjustment().upper()));
    f.memory.set_value(mem);
    f.disk.set_value(disk);
}

/// A card-style button: an OS tile, a name, a note and an end icon.
fn os_card(end_icon: &str) -> (gtk::Button, gtk::Image, gtk::Label, gtk::Label) {
    let b = gtk::Button::new();
    b.add_css_class("create-image");
    let c = widgets::hbox(12);
    let icon = gtk::Image::new();
    icon.set_pixel_size(36);
    c.append(&icon);
    let text = widgets::vbox(1);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title = widgets::label("", "create-image-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);
    let note = widgets::label("", "create-image-note");
    note.set_xalign(0.0);
    note.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&note);
    c.append(&text);
    let end = gtk::Image::from_icon_name(end_icon);
    end.add_css_class("dim");
    c.append(&end);
    b.set_child(Some(&c));
    (b, icon, title, note)
}

fn select_os(f: &Rc<Form>, alias: &str) {
    let Some((a, title, note, icon)) = IMAGES.iter().find(|i| i.0 == alias) else { return };
    *f.image.borrow_mut() = a.to_string();
    f.iso.borrow_mut().take();
    f.os_icon.set_icon_name(Some(icon));
    f.os_title.set_text(title);
    f.os_note.set_text(note);
    f.os_button.add_css_class("selected");
    f.iso_button.remove_css_class("selected");
    f.iso_note.set_text("Windows or any installer");
    let current = f.name.text().to_string();
    if current.is_empty() || IMAGES.iter().any(|i| current == default_name(i.0)) || current.ends_with("-1") {
        f.name.set_text(&default_name(a));
    }
}

fn default_name(alias: &str) -> String {
    let base = alias.split('/').next().unwrap_or("guest");
    if alias.ends_with("/desktop") { format!("{base}-desktop") } else { format!("{base}-1") }
}

fn wire_picker(f: &Rc<Form>) {
    let ff = f.clone();
    f.os_button.connect_clicked(move |b| {
        let popover = gtk::Popover::new();
        popover.add_css_class("create-os-menu");
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        let list = widgets::vbox(2);
        let vm = *ff.what.borrow() == What::Vm;
        for (alias, title, note, icon) in IMAGES.iter().filter(|i| vm || !VM_ONLY.contains(&i.0)) {
            let row = gtk::Button::new();
            row.add_css_class("flat");
            row.add_css_class("create-os-row");
            let c = widgets::hbox(12);
            let i = gtk::Image::from_icon_name(icon);
            i.set_pixel_size(30);
            c.append(&i);
            let text = widgets::vbox(1);
            let t = widgets::label(title, "create-image-title");
            t.set_xalign(0.0);
            text.append(&t);
            let n = widgets::label(note, "create-image-note");
            n.set_xalign(0.0);
            text.append(&n);
            c.append(&text);
            if *ff.image.borrow() == *alias && ff.iso.borrow().is_none() {
                let check = gtk::Image::from_icon_name("object-select-symbolic");
                check.set_hexpand(true);
                check.set_halign(gtk::Align::End);
                check.add_css_class("accent");
                c.append(&check);
            }
            row.set_child(Some(&c));
            let (f2, a, p) = (ff.clone(), alias.to_string(), popover.clone());
            row.connect_clicked(move |_| {
                select_os(&f2, &a);
                p.popdown();
            });
            list.append(&row);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(420)
            .child(&list)
            .build();
        popover.set_child(Some(&scroll));
        popover.set_parent(b);
        let w = b.width();
        list.set_size_request(w.max(280) - 12, -1);
        popover.connect_closed(|p| {
            let p = p.clone();
            gtk::glib::idle_add_local_once(move || p.unparent());
        });
        popover.popup();
    });
    let ff = f.clone();
    f.iso_button.connect_clicked(move |_| {
        let chooser = gtk::FileDialog::new();
        chooser.set_title("Installer ISO");
        let f2 = ff.clone();
        chooser.open(window::window().as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
            if let Some(path) = r.ok().and_then(|file| file.path()) {
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                *f2.iso.borrow_mut() = Some(path.display().to_string());
                f2.iso_button.add_css_class("selected");
                f2.os_button.remove_css_class("selected");
                f2.iso_note.set_text(&path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                let short: String = stem.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                if f2.name.text().is_empty() || f2.name.text().ends_with("-1") {
                    f2.name.set_text(if short.is_empty() { "vm-1" } else { &short });
                }
            }
        });
    });
    select_os(f, "ubuntu/24.04");
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !name.ends_with('-')
}

fn spec(f: &Form) -> Result<CreateSpec, String> {
    let what = *f.what.borrow();
    let name = f.name.text().trim().to_string();
    let host = *f.host.borrow();
    if host == HostId::Remote && !prefs::get().remote.configured() {
        return Err("Set up the remote host on Connections first.".into());
    }
    match what {
        What::Vm | What::Lxc => {
            if !valid_name(&name) {
                return Err("Names start with a letter and use letters, digits and dashes.".into());
            }
            let vm = what == What::Vm;
            let image = f.image.borrow().clone();
            let projects = crate::paths::home().join("Projects");
            let shares = if f.share.is_active() {
                vec![(projects.display().to_string(), projects.display().to_string())]
            } else {
                Vec::new()
            };
            Ok(CreateSpec {
                host,
                engine: Engine::Incus,
                kind: if vm { Kind::VirtualMachine } else { Kind::Container },
                name,
                image,
                start: f.start.is_active(),
                from_server: true,
                iso: if vm { f.iso.borrow().clone() } else { None },
                cpus: Some(f.cpus.value() as u32),
                memory_gib: Some(f.memory.value()),
                disk_gib: Some(f.disk.value() as u32),
                shares,
                ..Default::default()
            })
        }
        What::Docker => {
            let image = f.docker_image.text().trim().to_string();
            if image.is_empty() {
                return Err("Which image? For example nginx:latest.".into());
            }
            let typed = f.docker_name.text().trim().to_string();
            let name = if typed.is_empty() {
                image.rsplit('/').next().unwrap_or(&image).split(':').next().unwrap_or("app").to_string()
            } else {
                typed
            };
            let mut ports = Vec::new();
            for part in f.ports.text().split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let (h, g) = part.split_once(':').unwrap_or((part, part));
                match (h.trim().parse(), g.trim().parse()) {
                    (Ok(h), Ok(g)) => ports.push((h, g)),
                    _ => return Err(format!("“{part}” isn't a port pair like 8080:80.")),
                }
            }
            let home = crate::paths::home().display().to_string();
            let mut volumes = Vec::new();
            for part in f.volumes.text().split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let Some((h, g)) = part.split_once(':') else {
                    return Err(format!("“{part}” needs host:path."));
                };
                let h = h.trim().strip_prefix('~').map(|r| format!("{home}{r}")).unwrap_or_else(|| h.trim().to_string());
                volumes.push((h, g.trim().to_string()));
            }
            let buffer = f.env.buffer();
            let env: Vec<String> = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect();
            if let Some(bad) = env.iter().find(|l| !l.contains('=')) {
                return Err(format!("“{bad}” needs KEY=value."));
            }
            let restart = ["no", "unless-stopped", "always", "on-failure"].get(f.restart.selected() as usize).copied().unwrap_or("no");
            Ok(CreateSpec {
                host,
                engine: Engine::Docker,
                kind: Kind::Container,
                name,
                image,
                start: true,
                ports,
                volumes,
                env,
                restart: restart.into(),
                ..Default::default()
            })
        }
        What::Stack => Err(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_incus_rules() {
        assert!(valid_name("dev-box"));
        assert!(!valid_name("1box"));
        assert!(!valid_name("box_1"));
        assert!(!valid_name("box-"));
    }
}
