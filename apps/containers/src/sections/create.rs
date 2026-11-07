//! New workload: a VM, an LXC container or a Docker container.

use crate::models::{CreateSpec, Engine, HostId, Kind};
use crate::{prefs, widgets, window, worker};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn open(kind: Option<Kind>) {
    let (dialog, card) =
        widgets::dialog(if kind == Some(Kind::VirtualMachine) { "Create virtual machine" } else { "Create container" }, 480);
    let host = Rc::new(RefCell::new(HostId::Local));
    let engine = Rc::new(RefCell::new(if kind == Some(Kind::VirtualMachine) { Engine::Incus } else { Engine::Docker }));
    let kind_cell = Rc::new(RefCell::new(kind.unwrap_or(Kind::Container)));
    card.append(&widgets::segmented_row(
        "Host",
        "Where the workload will run.",
        vec![("local".into(), "Local".into()), ("remote".into(), window::host_name(HostId::Remote))],
        "local",
        {
            let host = host.clone();
            move |id| *host.borrow_mut() = if id == "remote" { HostId::Remote } else { HostId::Local }
        },
    ));
    card.append(&widgets::segmented_row(
        "Engine",
        "Docker containers, or Incus containers and VMs.",
        vec![("docker".into(), "Docker".into()), ("incus".into(), "Incus".into())],
        if kind == Some(Kind::VirtualMachine) { "incus" } else { "docker" },
        {
            let engine = engine.clone();
            let kind_cell = kind_cell.clone();
            move |id| {
                *engine.borrow_mut() = if id == "incus" { Engine::Incus } else { Engine::Docker };
                if *engine.borrow() == Engine::Docker {
                    *kind_cell.borrow_mut() = Kind::Container;
                }
            }
        },
    ));
    if kind != Some(Kind::VirtualMachine) {
        card.append(&widgets::segmented_row(
            "Type",
            "Virtual machines need Incus.",
            vec![("container".into(), "Container".into()), ("vm".into(), "Virtual machine".into())],
            "container",
            {
                let engine = engine.clone();
                let kind_cell = kind_cell.clone();
                move |id| {
                    if id == "vm" {
                        *kind_cell.borrow_mut() = Kind::VirtualMachine;
                        *engine.borrow_mut() = Engine::Incus;
                    } else {
                        *kind_cell.borrow_mut() = Kind::Container;
                    }
                }
            },
        ));
    }
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Name"));
    name.set_hexpand(true);
    card.append(&widgets::row("Name", "", Some(name.upcast_ref())));
    let image = gtk::Entry::new();
    image.set_placeholder_text(Some(if kind == Some(Kind::VirtualMachine) { "ubuntu/24.04" } else { "nginx:latest" }));
    image.set_hexpand(true);
    card.append(&widgets::row("Image", "Docker tag, or an Incus alias already on the host.", Some(image.upcast_ref())));
    let start = gtk::Switch::new();
    start.set_active(true);
    card.append(&widgets::row("Start after create", "", Some(start.upcast_ref())));
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
        let img = image.text().trim().to_string();
        if n.is_empty() || img.is_empty() {
            window::toast("Name and image are required.");
            return;
        }
        let spec = CreateSpec {
            host: *host.borrow(),
            engine: *engine.borrow(),
            kind: *kind_cell.borrow(),
            name: n,
            image: img,
            start: start.is_active(),
        };
        if spec.engine == Engine::Docker && spec.kind == Kind::VirtualMachine {
            window::toast("Docker does not run virtual machines. Choose Incus.");
            return;
        }
        if spec.host == HostId::Remote && !prefs::get().remote.configured() {
            window::toast("Configure a remote host first.");
            return;
        }
        window::toast(&format!("Creating {} on {}…", spec.name, window::host_name(spec.host)));
        worker::create(spec);
        d.close();
    });
    dialog.present();
}
