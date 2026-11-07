//! Every page in the sidebar, in order.

use crate::widgets::Page;

pub mod actions;
pub mod cards;
pub mod connections;
pub mod create;
pub mod diagnose;
pub mod host;
pub mod images;
pub mod networks;
pub mod session;
pub mod settings;
pub mod stacks;
pub mod storage;

#[derive(Clone)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    pub build: fn(&Page),
    pub fill: bool,
    pub nav: bool,
}

pub fn all() -> Vec<Section> {
    let s = |id, title, icon, group, description, build, fill| Section { id, title, icon, group, description, build, fill, nav: true };
    vec![
        s("overview", "Overview", "view-grid-symbolic", "", "Every VM, container and stack.", cards::build_overview, false),
        s("vms", "Virtual machines", "computer-symbolic", "Workloads", "Incus virtual machines.", cards::build_vms, false),
        s("lxc", "Containers (LXC)", "package-x-generic-symbolic", "Workloads", "Incus system containers.", cards::build_lxc, false),
        s("docker", "Docker", "application-x-executable-symbolic", "Workloads", "Docker containers.", cards::build_docker, false),
        s("stacks", "Stacks", "view-list-symbolic", "Workloads", "Docker Compose projects.", cards::build_stacks, false),
        s("images", "Images", "media-optical-symbolic", "Resources", "Container and VM images.", images::build, true),
        s("storage", "Storage", "drive-harddisk-symbolic", "Resources", "Volumes and storage pools.", storage::build, true),
        s("networks", "Networks", "network-wired-symbolic", "Resources", "Docker and Incus networks.", networks::build, true),
        s("host", "This computer", "computer-symbolic", "Hosts", "This computer's resources and engines.", host::build_local, false),
        s("homelab", "Remote", "network-server-symbolic", "Hosts", "The remote server's resources and engines.", host::build_remote, false),
        Section {
            nav: false,
            ..s(
                "connections",
                "Connections",
                "preferences-system-network-symbolic",
                "Hosts",
                "Local engines and the remote server.",
                connections::build,
                false,
            )
        },
    ]
}
