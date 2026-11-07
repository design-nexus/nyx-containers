//! A VM's screen inside its tab.

use crate::models::{HostId, RunState, Workload};
use crate::{widgets, window};
use gtk::prelude::*;

pub struct Display {
    pub root: gtk::Box,
}

impl Display {
    pub fn new(w: &Workload) -> std::rc::Rc<Display> {
        let root = widgets::vbox(0);
        root.add_css_class("tab-pad");
        let body = if w.state != RunState::Running {
            let w2 = w.clone();
            widgets::empty_state(
                "video-display-symbolic",
                &format!("{} is off", w.name),
                "Start it to see its screen here.",
                Some(("Start", Box::new(move || crate::sections::actions::run(&w2, crate::models::Action::Start)))),
            )
        } else {
            let (name, host) = (w.name.clone(), w.host);
            widgets::empty_state(
                "video-display-symbolic",
                "Display",
                "Open the VM's screen in a viewer window.",
                Some(("Open the display", Box::new(move || open_viewer(host, &name)))),
            )
        };
        root.append(&body);
        std::rc::Rc::new(Display { root })
    }

    pub fn focus(&self) {}

    pub fn close(&self) {}
}

pub fn open_viewer(host: HostId, name: &str) {
    if host != HostId::Local {
        window::toast("The remote VM's display opens from this computer's Incus client: run incus console --type=vga.");
        return;
    }
    match std::process::Command::new("incus").args(["console", name, "--type=vga"]).spawn() {
        Ok(_) => window::toast(&format!("Opening {name}'s display…")),
        Err(e) => window::toast(&format!("Couldn't start the viewer: {e}")),
    }
}
