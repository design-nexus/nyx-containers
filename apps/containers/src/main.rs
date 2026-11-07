//! Containers — Incus and Docker management for Omarchy.

mod cmd;
mod engines;
mod history;
mod models;
mod paths;
mod prefs;
mod sections;
mod security;
mod services;
mod settings_dialog;
mod tabs;
mod theme;
mod widgets;
mod window;
mod worker;

use gtk::prelude::*;
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.design_nexus.nyx.Containers";

const USAGE: &str = "Usage: nyx-containers [OPTIONS]\n\
\n\
  --section ID    open (or switch the open window) to a page: overview, vms, lxc,\n\
                  docker, stacks, images, storage, networks, host, connections\n\
  --open NAME     open a workload or stack in a tab\n\
  --view VIEW     with --open: summary, display, terminal, logs, snapshots, compose\n\
  --new           open the New workload dialog\n\
  --toggle        close the window if it's open, otherwise open it\n";

fn sections_create_open() {
    sections::create::open(None);
}

fn main() -> glib::ExitCode {
    nyx_core::app::init(nyx_core::app::App { slug: "nyx-containers", name: "Containers" });
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: still single-threaded; nothing else reads the environment yet.
        unsafe { std::env::set_var("GSK_RENDERER", "ngl") };
    }
    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).try_init();

    let id = std::env::var("NYX_CONTAINERS_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    let app = gtk::Application::builder().application_id(id).flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE).build();
    app.connect_command_line(|app, cl| {
        let argv: Vec<String> = cl.arguments().iter().map(|a| a.to_string_lossy().to_string()).collect();
        if argv.iter().any(|a| a == "--toggle")
            && let Some(win) = app.windows().into_iter().find(|w| !w.is_modal())
        {
            win.close();
            return glib::ExitCode::SUCCESS;
        }
        let section = argv.windows(2).find(|w| w[0] == "--section").map(|w| w[1].as_str());
        window::present(app, section);
        if argv.iter().any(|a| a == "--new") {
            // After the window shows: the card lives inside it.
            glib::idle_add_local_once(sections_create_open);
        }
        if let Some(name) = argv.windows(2).find(|w| w[0] == "--open").map(|w| w[1].clone()) {
            let view = argv.windows(2).find(|w| w[0] == "--view").map(|w| w[1].clone());
            window::open_by_name(&name, view.as_deref());
        }
        glib::ExitCode::SUCCESS
    });
    app.connect_shutdown(|_| {
        worker::shutdown();
        prefs::flush();
    });
    app.run()
}
