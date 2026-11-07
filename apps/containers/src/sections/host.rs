//! A host's page: how busy it is, its engines, whether it can run VMs well, its
//! storage pools, and what Docker could clean up.

use crate::models::{ConnState, HostHealth, HostId, HostInfo, Prune, Snapshot, Usage};
use crate::sections::cards::short_bytes;
use crate::widgets::{self, Page};
use crate::{window, worker};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct Ring {
    area: gtk::DrawingArea,
    value: Rc<Cell<f64>>,
    label: gtk::Label,
    sub: gtk::Label,
}

struct HostPage {
    host: HostId,
    stack: gtk::Stack,
    rings: Vec<Ring>,
    chart: gtk::DrawingArea,
    chart_data: Rc<RefCell<(Vec<f64>, Vec<f64>)>>,
    engines: gtk::Box,
    checks: gtk::Box,
    storage: gtk::Box,
    cleanup: gtk::Box,
    cleanup_sig: RefCell<String>,
}

thread_local! {
    static UI: RefCell<Vec<Rc<HostPage>>> = const { RefCell::new(Vec::new()) };
    static HEALTH: RefCell<Vec<HostHealth>> = const { RefCell::new(Vec::new()) };
    static INFO: RefCell<Vec<HostInfo>> = const { RefCell::new(Vec::new()) };
}

/// This computer's memory, from the last sample.
pub fn host_memory() -> Option<u64> {
    HEALTH.with(|h| h.borrow().iter().find(|x| x.host == HostId::Local).and_then(|x| x.mem_total))
}

pub fn build_local(page: &Page) {
    build(page, HostId::Local);
}

pub fn build_remote(page: &Page) {
    build(page, HostId::Remote);
}

fn card(title: &str, icon: &str) -> (gtk::Box, gtk::Box) {
    let c = widgets::vbox(0);
    c.add_css_class("tab-card");
    c.add_css_class("host-card");
    let head = widgets::hbox(8);
    head.add_css_class("tab-card-head");
    let i = gtk::Image::from_icon_name(icon);
    i.add_css_class("accent");
    head.append(&i);
    head.append(&widgets::label(title, "tab-card-title"));
    c.append(&head);
    let body = widgets::vbox(4);
    body.add_css_class("tab-card-body");
    c.append(&body);
    (c, body)
}

fn ring(title: &str) -> Ring {
    let area = gtk::DrawingArea::new();
    area.set_content_width(76);
    area.set_content_height(76);
    area.add_css_class("host-ring");
    let value = Rc::new(Cell::new(0.0_f64));
    let v = value.clone();
    area.set_draw_func(move |a, cr, w, h| {
        let c = a.color();
        let (cx, cy, r) = (w as f64 / 2.0, h as f64 / 2.0, (w.min(h) as f64 / 2.0) - 5.0);
        cr.set_line_width(6.0);
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, 0.16);
        cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();
        let frac = v.get().clamp(0.0, 1.0);
        if frac > 0.0 {
            cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, 1.0);
            cr.set_line_cap(gtk::cairo::LineCap::Round);
            let start = -std::f64::consts::FRAC_PI_2;
            cr.arc(cx, cy, r, start, start + frac * std::f64::consts::TAU);
            let _ = cr.stroke();
        }
    });
    let label = widgets::label("—", "host-ring-value");
    let _ = title;
    let sub = widgets::label("", "host-ring-sub");
    Ring { area, value, label, sub }
}

fn build(page: &Page, host: HostId) {
    page.body.add_css_class("host-page");
    let stack = gtk::Stack::new();
    let empty = widgets::empty_state(
        "network-server-symbolic",
        "No remote host yet",
        "Add a home server or another computer: Docker over SSH, Incus over HTTPS.",
        Some(("Add a remote", Box::new(|| window::navigate("connections")))),
    );
    stack.add_named(&empty, Some("empty"));
    let content = widgets::vbox(12);

    let flow = |cards: &[&gtk::Box]| {
        let f = gtk::FlowBox::new();
        f.set_selection_mode(gtk::SelectionMode::None);
        f.set_max_children_per_line(2);
        f.set_min_children_per_line(1);
        f.set_homogeneous(true);
        f.set_column_spacing(12);
        f.set_row_spacing(12);
        for c in cards {
            f.append(*c);
        }
        f
    };

    let (res_card, res_body) = card("Resources", "computer-symbolic");
    let rings_row = widgets::hbox(0);
    rings_row.set_homogeneous(true);
    let mut rings = Vec::new();
    for title in ["CPU", "Memory", "Disk"] {
        let r = ring(title);
        let col = widgets::vbox(4);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&r.area));
        r.label.set_halign(gtk::Align::Center);
        r.label.set_valign(gtk::Align::Center);
        overlay.add_overlay(&r.label);
        overlay.set_halign(gtk::Align::Center);
        col.append(&overlay);
        col.append(&widgets::label(title, "host-ring-title"));
        col.append(&r.sub);
        rings_row.append(&col);
        rings.push(r);
    }
    res_body.append(&rings_row);

    let (chart_card, chart_body) = card("Last few minutes", "utilities-system-monitor-symbolic");
    let chart = gtk::DrawingArea::new();
    chart.set_content_height(118);
    chart.set_hexpand(true);
    chart.add_css_class("host-chart");
    let chart_data: Rc<RefCell<(Vec<f64>, Vec<f64>)>> = Rc::new(RefCell::new((Vec::new(), Vec::new())));
    let d = chart_data.clone();
    chart.set_draw_func(move |a, cr, w, h| {
        let c = a.color();
        let (r, g, b) = (c.red() as f64, c.green() as f64, c.blue() as f64);
        let (w, h) = (w as f64, h as f64);
        cr.set_line_width(1.0);
        cr.set_source_rgba(r, g, b, 0.08);
        for f in [0.25, 0.5, 0.75] {
            cr.move_to(0.0, (h * f).round() + 0.5);
            cr.line_to(w, (h * f).round() + 0.5);
        }
        let _ = cr.stroke();
        let (cpu, mem) = &*d.borrow();
        let line = |vals: &[f64], fill: bool, dash: bool| {
            if vals.len() < 2 {
                return;
            }
            let step = w / (vals.len() as f64 - 1.0);
            let y = |v: f64| h - 2.0 - v.clamp(0.0, 1.0) * (h - 4.0);
            cr.move_to(0.0, y(vals[0]));
            for (i, v) in vals.iter().enumerate().skip(1) {
                cr.line_to(step * i as f64, y(*v));
            }
            if fill {
                let path = cr.copy_path();
                cr.line_to(w, h);
                cr.line_to(0.0, h);
                cr.close_path();
                cr.set_source_rgba(r, g, b, 0.14);
                let _ = cr.fill();
                if let Ok(p) = path {
                    cr.append_path(&p);
                }
            }
            cr.set_source_rgba(r, g, b, if dash { 0.65 } else { 1.0 });
            cr.set_line_width(2.0);
            if dash {
                cr.set_dash(&[5.0, 4.0], 0.0);
            }
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        };
        line(cpu, true, false);
        line(mem, false, true);
    });
    chart_body.append(&chart);
    let legend = widgets::label("CPU (solid) · Memory (dashed)", "host-legend");
    legend.set_xalign(0.0);
    chart_body.append(&legend);

    content.append(&flow(&[&res_card, &chart_card]));

    let (eng_card, engines) = card("Engines", "package-x-generic-symbolic");
    let (chk_card, checks) = card("VM features", "security-high-symbolic");
    content.append(&flow(&[&eng_card, &chk_card]));

    let (sto_card, storage) = card("Storage pools", "drive-harddisk-symbolic");
    let (cln_card, cleanup) = card("Clean up Docker", "edit-clear-all-symbolic");
    content.append(&flow(&[&sto_card, &cln_card]));

    let show = gtk::Button::with_label("Show what runs here");
    show.set_halign(gtk::Align::Start);
    show.connect_clicked(move |_| {
        window::set_host(if host == HostId::Local { crate::models::HostFilter::Local } else { crate::models::HostFilter::Remote });
        window::navigate("overview");
    });
    content.append(&show);
    stack.add_named(&content, Some("content"));
    page.body.append(&stack);

    let ui = Rc::new(HostPage {
        host,
        stack,
        rings,
        chart,
        chart_data,
        engines,
        checks,
        storage,
        cleanup,
        cleanup_sig: RefCell::new(String::new()),
    });
    paint_checks(&ui);
    UI.with(|u| u.borrow_mut().push(ui.clone()));
    // The worker may not be running yet when the first page builds.
    gtk::glib::idle_add_local_once(move || worker::host_info(host));
    let weak = Rc::downgrade(&ui);
    gtk::glib::timeout_add_seconds_local(30, move || match weak.upgrade() {
        Some(ui) => {
            if ui.stack.is_mapped() {
                worker::host_info(ui.host);
            }
            gtk::glib::ControlFlow::Continue
        }
        None => gtk::glib::ControlFlow::Break,
    });
    if let Some(snap) = window::snapshot() {
        refresh(&snap);
    }
}

pub fn apply_health(list: Vec<HostHealth>) {
    HEALTH.with(|h| *h.borrow_mut() = list);
    for ui in UI.with(|u| u.borrow().clone()) {
        paint_health(&ui);
    }
}

pub fn apply_info(info: HostInfo) {
    INFO.with(|i| {
        let mut list = i.borrow_mut();
        list.retain(|x| x.host != info.host);
        list.push(info);
    });
    for ui in UI.with(|u| u.borrow().clone()) {
        paint_info(&ui);
    }
}

pub fn refresh(snapshot: &Snapshot) {
    for ui in UI.with(|u| u.borrow().clone()) {
        let configured = ui.host == HostId::Local || crate::prefs::get().remote.configured();
        ui.stack.set_visible_child_name(if configured { "content" } else { "empty" });
        clear(&ui.engines);
        let engines: Vec<_> = snapshot.engines.iter().filter(|e| e.host == ui.host).collect();
        if engines.is_empty() {
            ui.engines.append(&widgets::label("Looking for Docker and Incus…", "dim"));
        }
        for st in engines {
            let (dot, detail) = match &st.state {
                ConnState::Connected { detail } => ("run", detail.clone()),
                ConnState::Unavailable { reason } | ConnState::Permission { reason } | ConnState::Insecure { reason } => ("err", reason.clone()),
                ConnState::Unconfigured => ("off", "Not set up for this host".into()),
                ConnState::Untrusted { .. } => ("err", "Its identity isn't trusted yet".into()),
            };
            let count = snapshot.workloads.iter().filter(|w| w.host == ui.host && w.engine == st.engine).count();
            let row = widgets::hbox(10);
            row.add_css_class("host-engine-row");
            let d = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            d.add_css_class("svc-dot");
            d.add_css_class(dot);
            d.set_valign(gtk::Align::Center);
            row.append(&d);
            let text = widgets::vbox(1);
            text.set_hexpand(true);
            let title = widgets::label(st.engine.label(), "host-engine-name");
            title.set_xalign(0.0);
            text.append(&title);
            let sub = widgets::label(&format!("{detail} · {count} workload{}", if count == 1 { "" } else { "s" }), "dim host-engine-sub");
            sub.set_xalign(0.0);
            sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&sub);
            row.append(&text);
            if dot == "err" {
                let fix = gtk::Button::with_label("Fix");
                fix.connect_clicked(|_| crate::settings_dialog::open_diagnostics());
                row.append(&fix);
            }
            ui.engines.append(&row);
        }
        paint_health(&ui);
    }
}

fn paint_health(ui: &HostPage) {
    let Some(h) = HEALTH.with(|x| x.borrow().iter().find(|x| x.host == ui.host).cloned()) else { return };
    let frac = |u: Option<u64>, t: Option<u64>| match (u, t) {
        (Some(u), Some(t)) if t > 0 => Some(u as f64 / t as f64),
        _ => None,
    };
    let set = |r: &Ring, f: Option<f64>, sub: String| {
        r.value.set(f.unwrap_or(0.0));
        r.label.set_text(&f.map(|f| format!("{:.0}%", f * 100.0)).unwrap_or_else(|| "—".into()));
        r.sub.set_text(&sub);
        r.area.queue_draw();
    };
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    set(&ui.rings[0], h.cpu.map(|c| c / 100.0), if ui.host == HostId::Local { format!("{cores} cores") } else { String::new() });
    set(
        &ui.rings[1],
        frac(h.mem_used, h.mem_total),
        match (h.mem_used, h.mem_total) {
            (Some(u), Some(t)) => format!("{} of {}", short_bytes(u), short_bytes(t)),
            _ => String::new(),
        },
    );
    set(
        &ui.rings[2],
        frac(h.disk_used, h.disk_total),
        match (h.disk_used, h.disk_total) {
            (Some(u), Some(t)) => format!("{} of {}", short_bytes(u), short_bytes(t)),
            (Some(u), None) => format!("{} used", short_bytes(u)),
            _ => String::new(),
        },
    );
    *ui.chart_data.borrow_mut() = (h.cpu_hist.clone(), h.mem_hist.clone());
    ui.chart.queue_draw();
}

fn paint_info(ui: &Rc<HostPage>) {
    let Some(info) = INFO.with(|i| i.borrow().iter().find(|x| x.host == ui.host).cloned()) else { return };
    clear(&ui.storage);
    if info.pools.is_empty() {
        ui.storage.append(&widgets::label("No Incus storage pools.", "dim"));
    }
    for p in &info.pools {
        let row = widgets::vbox(5);
        row.add_css_class("host-pool");
        let top = widgets::hbox(8);
        let name = widgets::label(&p.name, "host-engine-name");
        name.set_hexpand(true);
        name.set_xalign(0.0);
        top.append(&name);
        let free = match (p.used, p.total) {
            (Some(u), Some(t)) if t >= u => format!("{} free of {}", short_bytes(t - u), short_bytes(t)),
            _ => String::new(),
        };
        top.append(&widgets::label(&free, "dim host-engine-sub"));
        row.append(&top);
        let sub = widgets::label(&format!("Incus · {}", p.driver), "dim host-engine-sub");
        sub.set_xalign(0.0);
        row.append(&sub);
        let bar = gtk::LevelBar::new();
        bar.add_css_class("host-bar");
        if let (Some(u), Some(t)) = (p.used, p.total)
            && t > 0
        {
            bar.set_value(u as f64 / t as f64);
        }
        row.append(&bar);
        ui.storage.append(&row);
    }
    if let Some(d) = &info.docker {
        let total = d.images.total + d.containers.total + d.volumes.total + d.build_cache.total;
        let row = widgets::hbox(8);
        row.add_css_class("host-pool");
        let name = widgets::label("Docker data", "host-engine-name");
        name.set_hexpand(true);
        name.set_xalign(0.0);
        row.append(&name);
        row.append(&widgets::label(&format!("{} in images, containers, volumes and cache", short_bytes(total)), "dim host-engine-sub"));
        ui.storage.append(&row);
    }

    let sig = format!("{:?}", info.docker.map(|d| (d.images.reclaimable, d.build_cache.reclaimable, d.containers.reclaimable, d.volumes.reclaimable)));
    if *ui.cleanup_sig.borrow() == sig {
        return;
    }
    *ui.cleanup_sig.borrow_mut() = sig;
    clear(&ui.cleanup);
    let Some(d) = info.docker else {
        ui.cleanup.append(&widgets::label("Docker isn't running on this host.", "dim"));
        return;
    };
    let host = ui.host;
    let row = |title: &str, desc: String, u: Usage, what: Prune, armed: &str| {
        let control: gtk::Widget = if u.reclaimable > 0 {
            let b = widgets::two_click(&format!("Free {}", short_bytes(u.reclaimable)), armed, move || {
                window::toast("Cleaning up…");
                worker::prune(host, what);
            });
            b.upcast()
        } else {
            widgets::label("Nothing to free", "dim").upcast()
        };
        widgets::row(title, &desc, Some(&control))
    };
    let list = widgets::vbox(0);
    list.add_css_class("host-cleanup");
    list.append(&row(
        "Unused images",
        format!("{} of {} images no container uses", d.images.count.saturating_sub(d.images.active), d.images.count),
        d.images,
        Prune::Images,
        "Click again to delete them",
    ));
    list.append(&row("Build cache", "Layers kept from image builds".into(), d.build_cache, Prune::BuildCache, "Click again to clear it"));
    list.append(&row(
        "Stopped containers",
        format!("{} stopped", d.containers.count.saturating_sub(d.containers.active)),
        d.containers,
        Prune::Containers,
        "Click again to delete them",
    ));
    list.append(&row(
        "Unnamed volumes",
        "Anonymous volumes no container uses; named volumes stay".into(),
        d.volumes,
        Prune::Volumes,
        "Click again: their data goes",
    ));
    ui.cleanup.append(&list);
}

/// Can this computer run VMs well? Each check names what's missing.
fn paint_checks(ui: &HostPage) {
    clear(&ui.checks);
    if ui.host != HostId::Local {
        ui.checks.append(&widgets::label("Checked on this computer only.", "dim"));
        return;
    }
    let exists = |p: &str| std::path::Path::new(p).exists();
    let on_path = |bin: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).exists()));
    let read = |p: &str| std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
    let nested = ["/sys/module/kvm_intel/parameters/nested", "/sys/module/kvm_amd/parameters/nested"].iter().any(|p| matches!(read(p).as_str(), "Y" | "1"));
    let iommu = std::fs::read_dir("/sys/kernel/iommu_groups").map(|d| d.count() > 0).unwrap_or(false);
    let checks = [
        ("KVM", exists("/dev/kvm"), "Hardware virtualization is off in the firmware, or KVM isn't loaded."),
        ("Nested virtualization", nested, "VMs can't run VMs or Docker Desktop inside."),
        ("IOMMU (GPU and USB passthrough)", iommu, "Turn on VT-d or AMD-Vi in the firmware, and intel_iommu=on."),
        ("virtiofs shared folders", on_path("virtiofsd") || exists("/usr/lib/virtiofsd") || exists("/usr/libexec/virtiofsd"), "Install virtiofsd for fast shared folders in VMs."),
        ("Virtual TPM", on_path("swtpm"), "Install swtpm for Windows 11 and measured boot."),
        ("UEFI firmware", ["/usr/share/edk2/x64/OVMF_CODE.4m.fd", "/usr/share/edk2/x64/OVMF_CODE.fd", "/usr/share/OVMF", "/usr/share/ovmf"].iter().any(|p| exists(p)), "Install edk2-ovmf for UEFI and Secure Boot."),
    ];
    for (title, ok, why) in checks {
        let row = widgets::hbox(10);
        row.add_css_class("host-check");
        let icon = gtk::Image::from_icon_name(if ok { "emblem-ok-symbolic" } else { "dialog-warning-symbolic" });
        icon.add_css_class(if ok { "ok" } else { "warn" });
        icon.set_valign(gtk::Align::Start);
        row.append(&icon);
        let text = widgets::vbox(1);
        let t = widgets::label(title, "");
        t.set_xalign(0.0);
        text.append(&t);
        if !ok {
            let d = widgets::label(why, "dim host-engine-sub");
            d.set_xalign(0.0);
            d.set_wrap(true);
            text.append(&d);
        }
        row.append(&text);
        ui.checks.append(&row);
    }
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}
