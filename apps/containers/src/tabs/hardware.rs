//! Hardware and Options for an Incus VM or container, Proxmox style: every device in
//! one list, the selected one's settings beside it. Edits go into a draft; Apply
//! writes the instance's config and devices in one go, Revert drops them.

use crate::engines::incus::InstanceConfig;
use crate::models::{HostId, Workload};
use crate::sections::cards::short_bytes;
use crate::{widgets, window, worker};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::{Rc, Weak};

type Devices = BTreeMap<String, BTreeMap<String, String>>;

pub struct Editor {
    host: HostId,
    name: String,
    orig: RefCell<Option<InstanceConfig>>,
    draft: RefCell<InstanceConfig>,
    selected: RefCell<String>,
    list: gtk::Box,
    form: gtk::Box,
    options: gtk::Box,
    pending: RefCell<Vec<gtk::Label>>,
    apply: RefCell<Vec<gtk::Button>>,
    revert: RefCell<Vec<gtk::Button>>,
    busy: Cell<bool>,
}

thread_local! {
    static EDITORS: RefCell<HashMap<String, Weak<Editor>>> = RefCell::new(HashMap::new());
}

fn key(host: HostId, name: &str) -> String {
    format!("{}|{name}", host.label())
}

pub fn editor(w: &Workload) -> Rc<Editor> {
    if let Some(e) = EDITORS.with(|m| m.borrow().get(&key(w.host, &w.name)).and_then(Weak::upgrade)) {
        return e;
    }
    let e = Rc::new(Editor {
        host: w.host,
        name: w.name.clone(),
        orig: RefCell::new(None),
        draft: RefCell::new(InstanceConfig::default()),
        selected: RefCell::new("cpu".into()),
        list: widgets::vbox(1),
        form: widgets::vbox(0),
        options: widgets::vbox(0),
        pending: RefCell::new(Vec::new()),
        apply: RefCell::new(Vec::new()),
        revert: RefCell::new(Vec::new()),
        busy: Cell::new(false),
    });
    EDITORS.with(|m| m.borrow_mut().insert(key(w.host, &w.name), Rc::downgrade(&e)));
    worker::load_config(w.host, w.name.clone());
    e
}

pub fn apply_config(host: HostId, name: &str, result: Result<InstanceConfig, String>) {
    let Some(e) = EDITORS.with(|m| m.borrow().get(&key(host, name)).and_then(Weak::upgrade)) else { return };
    e.busy.set(false);
    match result {
        Ok(cfg) => {
            *e.draft.borrow_mut() = cfg.clone();
            *e.orig.borrow_mut() = Some(cfg);
            e.repaint();
        }
        Err(err) => window::toast(&format!("Couldn't read {name}'s settings: {err}")),
    }
}

pub fn apply_failed(host: HostId, name: &str) {
    if let Some(e) = EDITORS.with(|m| m.borrow().get(&key(host, name)).and_then(Weak::upgrade)) {
        e.busy.set(false);
        e.paint_pending();
    }
}

fn footer(e: &Rc<Editor>) -> gtk::Box {
    let foot = widgets::hbox(8);
    foot.add_css_class("hw-foot");
    let pending = widgets::label("", "hw-pending");
    pending.set_hexpand(true);
    pending.set_xalign(0.0);
    pending.set_wrap(true);
    foot.append(&pending);
    let revert = gtk::Button::with_label("Revert");
    let apply = gtk::Button::with_label("Apply");
    apply.add_css_class("suggested-action");
    foot.append(&revert);
    foot.append(&apply);
    let w = Rc::downgrade(e);
    revert.connect_clicked(move |_| {
        if let Some(e) = w.upgrade()
            && let Some(orig) = e.orig.borrow().clone()
        {
            *e.draft.borrow_mut() = orig;
            e.repaint();
        }
    });
    let w = Rc::downgrade(e);
    apply.connect_clicked(move |_| {
        if let Some(e) = w.upgrade() {
            e.busy.set(true);
            e.paint_pending();
            window::toast(&format!("Saving {}…", e.name));
            worker::save_config(e.host, e.name.clone(), e.draft.borrow().clone());
        }
    });
    e.pending.borrow_mut().push(pending);
    e.apply.borrow_mut().push(apply);
    e.revert.borrow_mut().push(revert);
    foot
}

pub fn build_hardware(w: &Workload) -> (gtk::Widget, Rc<Editor>) {
    let e = editor(w);
    let root = widgets::hbox(12);
    root.add_css_class("tab-pad");
    root.add_css_class("hw-view");
    let left = widgets::vbox(0);
    left.add_css_class("tab-card");
    left.set_size_request(400, -1);
    let head = widgets::hbox(8);
    head.add_css_class("tab-card-head");
    head.append(&widgets::label("Devices", "tab-card-title"));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    head.append(&spacer);
    let add = gtk::Button::new();
    let add_box = widgets::hbox(6);
    add_box.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    add_box.append(&widgets::label("Add device", ""));
    add.set_child(Some(&add_box));
    add.add_css_class("flat");
    let weak = Rc::downgrade(&e);
    add.connect_clicked(move |b| {
        if let Some(e) = weak.upgrade() {
            nyx_ui::menu::popup_below(b, e.add_entries());
        }
    });
    head.append(&add);
    left.append(&head);
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&e.list).build();
    left.append(&scroll);
    root.append(&left);
    let right = widgets::vbox(0);
    right.add_css_class("tab-card");
    right.set_hexpand(true);
    let fscroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&e.form).build();
    right.append(&fscroll);
    right.append(&footer(&e));
    root.append(&right);
    e.repaint();
    (root.upcast(), e)
}

pub fn build_options(w: &Workload) -> (gtk::Widget, Rc<Editor>) {
    let e = editor(w);
    let root = widgets::vbox(0);
    root.add_css_class("tab-pad");
    let card = widgets::vbox(0);
    card.add_css_class("tab-card");
    card.set_vexpand(true);
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&e.options).build();
    card.append(&scroll);
    card.append(&footer(&e));
    root.append(&card);
    e.repaint();
    (root.upcast(), e)
}

fn host_cores() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

fn ranges(cores: &[usize]) -> String {
    let mut out = Vec::new();
    let mut i = 0;
    while i < cores.len() {
        let start = cores[i];
        let mut end = start;
        while i + 1 < cores.len() && cores[i + 1] == end + 1 {
            i += 1;
            end = cores[i];
        }
        out.push(if start == end { start.to_string() } else { format!("{start}-{end}") });
        i += 1;
    }
    out.join(",")
}

fn pinned(spec: &str) -> Vec<usize> {
    if spec.trim().parse::<u32>().is_ok() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for part in spec.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                out.extend(a..=b);
            }
        } else if let Ok(n) = part.trim().parse::<usize>() {
            out.push(n);
        }
    }
    out
}

fn label_for(k: &str) -> &str {
    match k {
        "size" => "Size",
        "path" => "Path in the guest",
        "source" => "Source",
        "pool" => "Storage pool",
        "network" => "Network",
        "parent" => "Host interface",
        "listen" => "Listen on the host",
        "connect" => "Connect to",
        "readonly" => "Read only",
        "shift" => "Map your user ID",
        "boot.priority" => "Boot priority",
        "vendorid" => "Vendor ID",
        "productid" => "Product ID",
        "gputype" => "GPU type",
        "pci" => "PCI address",
        "hwaddr" => "MAC address",
        "ipv4.address" => "Fixed IPv4 address",
        "nat" => "NAT",
        "required" => "Required to start",
        "name" => "Name in the guest",
        other => other,
    }
}

/// The kind of device, as people call it.
fn describe(name: &str, d: &BTreeMap<String, String>) -> (&'static str, String, String) {
    let get = |k: &str| d.get(k).cloned().unwrap_or_default();
    match d.get("type").map(String::as_str).unwrap_or("") {
        "disk" if get("path") == "/" => (
            "drive-harddisk-symbolic",
            "Root disk".into(),
            [get("size").parse::<u64>().ok().map(short_bytes).or(Some(get("size"))).filter(|s| !s.is_empty()), Some(format!("pool {}", get("pool")))]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        "disk" if get("source").to_ascii_lowercase().ends_with(".iso") => {
            ("media-optical-symbolic", "CD/DVD".into(), std::path::Path::new(&get("source")).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())
        }
        "disk" if !get("pool").is_empty() => ("drive-harddisk-symbolic", name.to_string(), format!("Volume {} · {}", get("source"), get("path"))),
        "disk" => ("folder-symbolic", "Shared folder".into(), format!("{} → {}", tilde(&get("source")), if get("path").is_empty() { "mount tag ".to_string() + name } else { get("path") })),
        "nic" => ("network-wired-symbolic", name.to_string(), [get("network"), get("parent"), get("hwaddr")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")),
        "proxy" => ("mail-forward-symbolic", "Port forward".into(), format!("{} → {}", get("listen"), get("connect"))),
        "usb" => ("media-removable-symbolic", "USB device".into(), format!("{}:{}", get("vendorid"), get("productid"))),
        "gpu" => ("video-display-symbolic", "GPU".into(), [get("gputype"), get("pci"), get("id")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")),
        "tpm" => ("security-high-symbolic", "TPM".into(), "Virtual TPM 2.0".into()),
        other => ("application-x-firmware-symbolic", name.to_string(), other.to_string()),
    }
}

fn tilde(path: &str) -> String {
    let home = crate::paths::home().display().to_string();
    path.strip_prefix(&home).map(|r| format!("~{r}")).unwrap_or_else(|| path.to_string())
}

impl Editor {
    fn changes(&self) -> usize {
        let Some(orig) = self.orig.borrow().clone() else { return 0 };
        let d = self.draft.borrow();
        let mut n = 0;
        for k in orig.config.keys().chain(d.config.keys()).collect::<std::collections::BTreeSet<_>>() {
            if orig.config.get(k) != d.config.get(k) {
                n += 1;
            }
        }
        for k in orig.devices.keys().chain(d.devices.keys()).collect::<std::collections::BTreeSet<_>>() {
            if orig.devices.get(k) != d.devices.get(k) {
                n += 1;
            }
        }
        n
    }

    fn paint_pending(&self) {
        let n = self.changes();
        let running = self.orig.borrow().as_ref().is_some_and(|o| o.running);
        let vm = self.orig.borrow().as_ref().is_some_and(|o| o.vm);
        let text = match (n, self.busy.get()) {
            (_, true) => "Saving…".to_string(),
            (0, _) => "No changes".to_string(),
            (n, _) if running && vm => format!("{n} change{} · some take effect when {} restarts", if n == 1 { "" } else { "s" }, self.name),
            (n, _) => format!("{n} change{}", if n == 1 { "" } else { "s" }),
        };
        for l in self.pending.borrow().iter() {
            l.set_text(&text);
        }
        for b in self.apply.borrow().iter().chain(self.revert.borrow().iter()) {
            b.set_sensitive(n > 0 && !self.busy.get());
        }
    }

    fn set_config(&self, k: &str, v: &str) {
        let mut d = self.draft.borrow_mut();
        if v.is_empty() {
            d.config.remove(k);
        } else {
            d.config.insert(k.to_string(), v.to_string());
        }
        drop(d);
        self.paint_pending();
    }

    fn config(&self, k: &str) -> String {
        let d = self.draft.borrow();
        d.config.get(k).or_else(|| d.expanded_config.get(k)).cloned().unwrap_or_default()
    }

    /// A device as the instance will see it: its own, else the one its profile adds.
    fn device(&self, name: &str) -> Option<BTreeMap<String, String>> {
        let d = self.draft.borrow();
        d.devices.get(name).cloned().or_else(|| d.expanded_devices.get(name).cloned())
    }

    fn set_device_key(&self, name: &str, k: &str, v: &str) {
        let Some(mut dev) = self.device(name) else { return };
        if v.is_empty() {
            dev.remove(k);
        } else {
            dev.insert(k.into(), v.into());
        }
        self.draft.borrow_mut().devices.insert(name.into(), dev);
        self.paint_pending();
    }

    fn all_devices(&self) -> Vec<(String, BTreeMap<String, String>, bool)> {
        let d = self.draft.borrow();
        let mut names: Vec<String> = d.devices.keys().chain(d.expanded_devices.keys()).cloned().collect();
        names.sort();
        names.dedup();
        let mut out: Vec<_> = names
            .into_iter()
            .filter_map(|n| {
                let own = d.devices.contains_key(&n);
                let dev = d.devices.get(&n).or_else(|| d.expanded_devices.get(&n)).cloned()?;
                // A device removed from the draft that the profile no longer adds either.
                Some((n, dev, own))
            })
            .collect();
        out.sort_by_key(|(n, d, _)| {
            let rank = match d.get("type").map(String::as_str) {
                Some("disk") if d.get("path").map(String::as_str) == Some("/") => 0,
                Some("disk") => 1,
                Some("nic") => 2,
                Some("proxy") => 3,
                _ => 4,
            };
            (rank, n.clone())
        });
        out
    }

    fn repaint(self: &Rc<Self>) {
        self.paint_list();
        self.paint_form();
        self.paint_options();
        self.paint_pending();
    }

    fn row(self: &Rc<Self>, id: &str, icon: &str, title: &str, value: &str, note: Option<&str>) -> gtk::Button {
        let b = gtk::Button::new();
        b.add_css_class("hw-row");
        if *self.selected.borrow() == id {
            b.add_css_class("selected");
        }
        let c = widgets::hbox(10);
        c.append(&gtk::Image::from_icon_name(icon));
        let t = widgets::label(title, "hw-row-title");
        t.set_size_request(118, -1);
        t.set_xalign(0.0);
        t.set_ellipsize(gtk::pango::EllipsizeMode::End);
        c.append(&t);
        let v = widgets::label(value, "hw-row-value");
        v.set_xalign(0.0);
        v.set_hexpand(true);
        v.set_ellipsize(gtk::pango::EllipsizeMode::End);
        c.append(&v);
        if let Some(n) = note {
            c.append(&widgets::tag(n, ""));
        }
        b.set_child(Some(&c));
        let (w, id) = (Rc::downgrade(self), id.to_string());
        b.connect_clicked(move |_| {
            if let Some(e) = w.upgrade() {
                *e.selected.borrow_mut() = id.clone();
                e.paint_list();
                e.paint_form();
            }
        });
        b
    }

    fn paint_list(self: &Rc<Self>) {
        clear(&self.list);
        if self.orig.borrow().is_none() {
            self.list.append(&widgets::label("Loading…", "dim hw-loading"));
            return;
        }
        self.list.append(&section("Compute"));
        let cpu = self.config("limits.cpu");
        let pins = pinned(&cpu);
        let cpu_text = if !pins.is_empty() {
            format!("{} cores · pinned {}", pins.len(), cpu)
        } else if cpu.is_empty() {
            "All host cores".into()
        } else {
            format!("{cpu} cores")
        };
        let allowance = self.config("limits.cpu.allowance");
        let cpu_text = if allowance.is_empty() { cpu_text } else { format!("{cpu_text} · limit {allowance}") };
        self.list.append(&self.row("cpu", "computer-symbolic", "Processors", &cpu_text, None));
        let mem = self.config("limits.memory");
        self.list.append(&self.row("mem", "media-flash-symbolic", "Memory", if mem.is_empty() { "No limit" } else { &mem }, None));
        let devices = self.all_devices();
        let mut last = "";
        for (name, dev, own) in devices {
            let group = match dev.get("type").map(String::as_str) {
                Some("disk") => "Storage",
                Some("nic") | Some("proxy") => "Network",
                _ => "Devices",
            };
            if group != last {
                self.list.append(&section(group));
                last = group;
            }
            let (icon, title, value) = describe(&name, &dev);
            let note = (!own).then(|| "Profile");
            self.list.append(&self.row(&format!("dev:{name}"), icon, &title, &value, note));
        }
    }

    fn paint_form(self: &Rc<Self>) {
        clear(&self.form);
        if self.orig.borrow().is_none() {
            return;
        }
        let selected = self.selected.borrow().clone();
        match selected.as_str() {
            "cpu" => self.cpu_form(),
            "mem" => self.mem_form(),
            id => {
                if let Some(name) = id.strip_prefix("dev:") {
                    self.device_form(name);
                }
            }
        }
    }

    fn form_head(&self, icon: &str, title: &str, extra: Option<gtk::Widget>) {
        let head = widgets::hbox(8);
        head.add_css_class("tab-card-head");
        let i = gtk::Image::from_icon_name(icon);
        i.add_css_class("accent");
        head.append(&i);
        head.append(&widgets::label(title, "tab-card-title"));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        head.append(&spacer);
        if let Some(x) = extra {
            head.append(&x);
        }
        self.form.append(&head);
    }

    fn cpu_form(self: &Rc<Self>) {
        self.form_head("computer-symbolic", "Processors", None);
        let cores = host_cores();
        let cpu = self.config("limits.cpu");
        let pins = pinned(&cpu);
        let count = gtk::SpinButton::with_range(1.0, cores as f64, 1.0);
        count.set_value(cpu.parse::<f64>().unwrap_or(if pins.is_empty() { cores as f64 } else { pins.len() as f64 }));
        count.set_sensitive(pins.is_empty());
        let w = Rc::downgrade(self);
        count.connect_value_changed(move |s| {
            if let Some(e) = w.upgrade() {
                e.set_config("limits.cpu", &(s.value() as u32).to_string());
                e.paint_list();
            }
        });
        self.form.append(&widgets::row("Cores", &format!("This computer has {cores}."), Some(count.upcast_ref())));

        let grid = gtk::FlowBox::new();
        grid.set_selection_mode(gtk::SelectionMode::None);
        grid.set_max_children_per_line(8);
        grid.set_min_children_per_line(4);
        grid.set_homogeneous(true);
        grid.add_css_class("hw-cores");
        let toggles: Rc<RefCell<Vec<gtk::ToggleButton>>> = Rc::new(RefCell::new(Vec::new()));
        for c in 0..cores {
            let t = gtk::ToggleButton::with_label(&c.to_string());
            t.add_css_class("hw-core");
            t.set_active(pins.contains(&c));
            let (w, all) = (Rc::downgrade(self), toggles.clone());
            t.connect_toggled(move |_| {
                let Some(e) = w.upgrade() else { return };
                let on: Vec<usize> = all.borrow().iter().enumerate().filter(|(_, t)| t.is_active()).map(|(i, _)| i).collect();
                let value = if on.is_empty() { e.orig.borrow().as_ref().and_then(|o| o.config.get("limits.cpu").cloned()).filter(|v| v.parse::<u32>().is_ok()).unwrap_or_else(|| "2".into()) } else { ranges(&on) };
                e.set_config("limits.cpu", &value);
                e.paint_list();
            });
            toggles.borrow_mut().push(t.clone());
            grid.append(&t);
        }
        let pin_row = widgets::vbox(8);
        pin_row.add_css_class("settings-option");
        pin_row.append(&row_text("Pin to cores", "Keep it on these host cores. Leave all off to let the host choose."));
        pin_row.append(&grid);
        self.form.append(&pin_row);

        let allowance = gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 100.0, 5.0);
        allowance.set_hexpand(true);
        allowance.set_size_request(200, -1);
        let current = self.config("limits.cpu.allowance").trim_end_matches('%').parse::<f64>().unwrap_or(100.0);
        allowance.set_value(current);
        allowance.set_draw_value(true);
        allowance.set_value_pos(gtk::PositionType::Right);
        allowance.set_format_value_func(|_, v| format!("{v:.0}%"));
        let w = Rc::downgrade(self);
        allowance.connect_value_changed(move |s| {
            if let Some(e) = w.upgrade() {
                let v = s.value().round();
                e.set_config("limits.cpu.allowance", &if v >= 100.0 { String::new() } else { format!("{v:.0}%") });
            }
        });
        self.form.append(&widgets::row("CPU limit", "The most it may use of its cores.", Some(allowance.upcast_ref())));

        let prio = self.config("limits.cpu.priority").parse::<i32>().unwrap_or(5);
        let current = if prio <= 3 { "low" } else if prio >= 8 { "high" } else { "normal" };
        let w = Rc::downgrade(self);
        let seg = widgets::segmented(
            &[("low".into(), "Low".into()), ("normal".into(), "Normal".into()), ("high".into(), "High".into())],
            current,
            move |id| {
                if let Some(e) = w.upgrade() {
                    e.set_config("limits.cpu.priority", match id.as_str() {
                        "low" => "2",
                        "high" => "9",
                        _ => "",
                    });
                }
            },
        );
        self.form.append(&widgets::row("Priority", "Who wins when the host is busy.", Some(seg.upcast_ref())));
    }

    fn mem_form(self: &Rc<Self>) {
        self.form_head("media-flash-symbolic", "Memory", None);
        let total = crate::sections::host::host_memory().unwrap_or(16 * 1024 * 1024 * 1024);
        let gib = 1024.0 * 1024.0 * 1024.0;
        let current = crate::models::parse_size(&self.config("limits.memory")).map(|b| b as f64 / gib).unwrap_or(0.0);
        let spin = gtk::SpinButton::with_range(0.0, (total as f64 / gib).floor(), 0.5);
        spin.set_digits(1);
        spin.set_value(current);
        let w = Rc::downgrade(self);
        spin.connect_value_changed(move |s| {
            if let Some(e) = w.upgrade() {
                let v = s.value();
                e.set_config("limits.memory", &if v <= 0.0 { String::new() } else if v.fract() == 0.0 { format!("{v:.0}GiB") } else { format!("{:.0}MiB", v * 1024.0) });
                e.paint_list();
            }
        });
        let unit = widgets::hbox(6);
        unit.append(&spin);
        unit.append(&widgets::label("GiB", "dim"));
        self.form.append(&widgets::row(
            "Memory",
            &format!("This computer has {}. 0 means no limit.", short_bytes(total)),
            Some(unit.upcast_ref()),
        ));
        let vm = self.orig.borrow().as_ref().is_some_and(|o| o.vm);
        if !vm {
            let swap = self.config("limits.memory.swap") != "false";
            let w = Rc::downgrade(self);
            let (row, _) = widgets::switch_row("Swap", "Let it swap when memory runs short.", swap, move |on| {
                if let Some(e) = w.upgrade() {
                    e.set_config("limits.memory.swap", if on { "" } else { "false" });
                }
            });
            self.form.append(&row);
        }
    }

    fn device_form(self: &Rc<Self>, name: &str) {
        let Some(dev) = self.device(name) else { return };
        let (icon, title, _) = describe(name, &dev);
        let root = dev.get("type").map(String::as_str) == Some("disk") && dev.get("path").map(String::as_str) == Some("/");
        let remove = (!root).then(|| {
            let b = widgets::two_click("Remove", "Click again to remove", {
                let (w, n) = (Rc::downgrade(self), name.to_string());
                move || {
                    if let Some(e) = w.upgrade() {
                        let from_profile = e.draft.borrow().expanded_devices.contains_key(&n) && !e.orig.borrow().as_ref().is_some_and(|o| o.devices.contains_key(&n));
                        if from_profile {
                            window::toast("This device comes from a profile; change the profile to remove it.");
                            return;
                        }
                        e.draft.borrow_mut().devices.remove(&n);
                        e.draft.borrow_mut().expanded_devices.remove(&n);
                        *e.selected.borrow_mut() = "cpu".into();
                        e.repaint();
                    }
                }
            });
            b.upcast::<gtk::Widget>()
        });
        self.form_head(icon, &title, remove);
        if !self.draft.borrow().devices.contains_key(name) {
            let note = widgets::label("Added by a profile. Changing it here gives this instance its own copy.", "dim hw-note");
            note.set_wrap(true);
            note.set_xalign(0.0);
            self.form.append(&note);
        }
        if root {
            let size = dev.get("size").cloned().unwrap_or_default();
            let entry = gtk::Entry::new();
            entry.set_text(&size);
            entry.set_placeholder_text(Some("e.g. 64GiB"));
            let (w, n) = (Rc::downgrade(self), name.to_string());
            entry.connect_changed(move |en| {
                if let Some(e) = w.upgrade() {
                    e.set_device_key(&n, "size", en.text().trim());
                }
            });
            self.form.append(&widgets::row("Size", "Disks can grow, not shrink.", Some(entry.upcast_ref())));
            self.form.append(&widgets::row("Storage pool", "", Some(widgets::label(dev.get("pool").map(String::as_str).unwrap_or("—"), "dim").upcast_ref())));
            return;
        }
        let bools = ["readonly", "shift", "nat", "required"];
        for (k, v) in dev.iter().filter(|(k, _)| k.as_str() != "type") {
            if bools.contains(&k.as_str()) {
                let (w, n, k2) = (Rc::downgrade(self), name.to_string(), k.clone());
                let (row, _) = widgets::switch_row(label_for(k), "", v == "true", move |on| {
                    if let Some(e) = w.upgrade() {
                        e.set_device_key(&n, &k2, if on { "true" } else { "false" });
                    }
                });
                self.form.append(&row);
                continue;
            }
            let entry = gtk::Entry::new();
            entry.set_text(v);
            entry.set_hexpand(true);
            entry.set_width_chars(28);
            if k == "hwaddr" {
                entry.set_editable(false);
            }
            let (w, n, k2) = (Rc::downgrade(self), name.to_string(), k.clone());
            entry.connect_changed(move |en| {
                if let Some(e) = w.upgrade() {
                    e.set_device_key(&n, &k2, en.text().trim());
                }
            });
            self.form.append(&widgets::row(label_for(k), "", Some(entry.upcast_ref())));
        }
        if dev.get("type").map(String::as_str) == Some("disk") && dev.get("pool").is_none() && !dev.contains_key("readonly") {
            let (w, n) = (Rc::downgrade(self), name.to_string());
            let (row, _) = widgets::switch_row("Read only", "The guest can read but not change it.", false, move |on| {
                if let Some(e) = w.upgrade() {
                    e.set_device_key(&n, "readonly", if on { "true" } else { "" });
                }
            });
            self.form.append(&row);
        }
    }

    fn add_device(self: &Rc<Self>, base: &str, dev: BTreeMap<String, String>) {
        let mut name = base.to_string();
        let mut i = 1;
        while self.device(&name).is_some() {
            i += 1;
            name = format!("{base}{i}");
        }
        self.draft.borrow_mut().devices.insert(name.clone(), dev);
        *self.selected.borrow_mut() = format!("dev:{name}");
        self.repaint();
    }

    fn add_entries(self: &Rc<Self>) -> Vec<nyx_ui::menu::Entry> {
        let vm = self.orig.borrow().as_ref().is_some_and(|o| o.vm);
        let mut out = Vec::new();
        let w = Rc::downgrade(self);
        out.push(nyx_ui::menu::item("Shared folder…", move || {
            let Some(e) = w.upgrade() else { return };
            let chooser = gtk::FileDialog::new();
            chooser.set_title("Folder to share");
            let w2 = Rc::downgrade(&e);
            chooser.select_folder(window::window().as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
                if let (Some(e), Ok(f)) = (w2.upgrade(), r)
                    && let Some(path) = f.path()
                {
                    let base = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "share".into());
                    let mut dev = BTreeMap::new();
                    dev.insert("type".into(), "disk".into());
                    dev.insert("source".into(), path.display().to_string());
                    dev.insert("path".into(), format!("/mnt/{base}"));
                    if !vm {
                        dev.insert("shift".into(), "true".into());
                    }
                    e.add_device(&sanitize(&base), dev);
                }
            });
        }).icon("folder-symbolic"));
        if vm {
            let w = Rc::downgrade(self);
            out.push(nyx_ui::menu::item("CD/DVD image (ISO)…", move || {
                let Some(e) = w.upgrade() else { return };
                let chooser = gtk::FileDialog::new();
                chooser.set_title("ISO image");
                let w2 = Rc::downgrade(&e);
                chooser.open(window::window().as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
                    if let (Some(e), Ok(f)) = (w2.upgrade(), r)
                        && let Some(path) = f.path()
                    {
                        let mut dev = BTreeMap::new();
                        dev.insert("type".into(), "disk".into());
                        dev.insert("source".into(), path.display().to_string());
                        dev.insert("boot.priority".into(), "10".into());
                        e.add_device("iso", dev);
                    }
                });
            }).icon("media-optical-symbolic"));
        } else {
            let w = Rc::downgrade(self);
            out.push(nyx_ui::menu::item("Port forward…", move || {
                let Some(e) = w.upgrade() else { return };
                let w2 = Rc::downgrade(&e);
                widgets::ask_text("Forward a port", "Host port, then the port inside (8080 or 8080:80).", "", "Add", move |text| {
                    let Some(e) = w2.upgrade() else { return };
                    let (h, g) = text.split_once(':').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or((text.trim().to_string(), text.trim().to_string()));
                    if h.parse::<u16>().is_err() || g.parse::<u16>().is_err() {
                        window::toast("Use a port number, or host:guest.");
                        return;
                    }
                    let mut dev = BTreeMap::new();
                    dev.insert("type".into(), "proxy".into());
                    dev.insert("listen".into(), format!("tcp:0.0.0.0:{h}"));
                    dev.insert("connect".into(), format!("tcp:127.0.0.1:{g}"));
                    e.add_device(&format!("port{h}"), dev);
                });
            }).icon("mail-forward-symbolic"));
        }
        let w = Rc::downgrade(self);
        out.push(nyx_ui::menu::item("Network card", move || {
            if let Some(e) = w.upgrade() {
                let network = window::snapshot()
                    .and_then(|s| s.networks.iter().find(|n| n.host == e.host && n.engine == crate::models::Engine::Incus && n.removable).map(|n| n.name.clone()))
                    .unwrap_or_else(|| "incusbr0".into());
                let mut dev = BTreeMap::new();
                dev.insert("type".into(), "nic".into());
                dev.insert("network".into(), network);
                e.add_device("eth", dev);
            }
        }).icon("network-wired-symbolic"));
        if self.host == HostId::Local {
            let usb = local_usb();
            if !usb.is_empty() {
                let mut entries = Vec::new();
                for (vendor, product, label) in usb {
                    let w = Rc::downgrade(self);
                    entries.push(nyx_ui::menu::item(&label, move || {
                        if let Some(e) = w.upgrade() {
                            let mut dev = BTreeMap::new();
                            dev.insert("type".into(), "usb".into());
                            dev.insert("vendorid".into(), vendor.clone());
                            dev.insert("productid".into(), product.clone());
                            e.add_device("usb", dev);
                        }
                    }));
                }
                out.push(nyx_ui::menu::sub("USB device", entries));
            }
        }
        let w = Rc::downgrade(self);
        out.push(nyx_ui::menu::item("GPU", move || {
            if let Some(e) = w.upgrade() {
                let mut dev = BTreeMap::new();
                dev.insert("type".into(), "gpu".into());
                if vm {
                    dev.insert("gputype".into(), "physical".into());
                }
                e.add_device("gpu", dev);
                window::toast(if vm { "A physical GPU passes through while the VM runs; the host loses it until then." } else { "The container shares this computer's GPU." });
            }
        }).icon("video-display-symbolic"));
        if vm {
            let w = Rc::downgrade(self);
            out.push(nyx_ui::menu::item("TPM", move || {
                if let Some(e) = w.upgrade() {
                    let mut dev = BTreeMap::new();
                    dev.insert("type".into(), "tpm".into());
                    e.add_device("tpm", dev);
                }
            }).icon("security-high-symbolic"));
        }
        out
    }

    fn paint_options(self: &Rc<Self>) {
        clear(&self.options);
        if self.orig.borrow().is_none() {
            self.options.append(&widgets::label("Loading…", "dim hw-loading"));
            return;
        }
        let vm = self.orig.borrow().as_ref().is_some_and(|o| o.vm);
        self.options.append(&section("Start up"));
        let switch = |e: &Rc<Editor>, title: &str, desc: &str, key: &'static str, default: bool| {
            let current = match e.config(key).as_str() {
                "" => default,
                v => v == "true" || v == "1",
            };
            let w = Rc::downgrade(e);
            let (row, _) = widgets::switch_row(title, desc, current, move |on| {
                if let Some(e) = w.upgrade() {
                    e.set_config(key, if on == default { "" } else if on { "true" } else { "false" });
                }
            });
            row
        };
        self.options.append(&switch(self, "Start with the host", "Starts when this computer boots.", "boot.autostart", false));
        let spin = |e: &Rc<Editor>, title: &str, desc: &str, key: &'static str, max: f64| {
            let s = gtk::SpinButton::with_range(0.0, max, 1.0);
            s.set_value(e.config(key).parse::<f64>().unwrap_or(0.0));
            let w = Rc::downgrade(e);
            s.connect_value_changed(move |s| {
                if let Some(e) = w.upgrade() {
                    let v = s.value() as i64;
                    e.set_config(key, &if v == 0 { String::new() } else { v.to_string() });
                }
            });
            widgets::row(title, desc, Some(s.upcast_ref()))
        };
        self.options.append(&spin(self, "Start order", "Higher starts first.", "boot.autostart.priority", 100.0));
        self.options.append(&spin(self, "Wait after starting", "Seconds before the next one starts.", "boot.autostart.delay", 600.0));
        self.options.append(&section("Security"));
        if vm {
            self.options.append(&switch(self, "Secure Boot", "Only boot signed systems. Turn off for most Linux ISOs that aren't signed.", "security.secureboot", true));
        } else {
            self.options.append(&switch(self, "Nesting", "Run Docker or containers inside it.", "security.nesting", false));
            self.options.append(&switch(self, "Privileged", "Root inside is root on the host. Only for things that need it.", "security.privileged", false));
        }
        self.options.append(&section("Snapshots"));
        let schedule = self.config("snapshots.schedule");
        let w = Rc::downgrade(self);
        let current = match schedule.as_str() {
            "" => "off",
            "@hourly" => "hourly",
            "@daily" => "daily",
            "@weekly" => "weekly",
            _ => "custom",
        };
        let mut opts = vec![("off".to_string(), "Off".to_string()), ("hourly".into(), "Hourly".into()), ("daily".into(), "Daily".into()), ("weekly".into(), "Weekly".into())];
        if current == "custom" {
            opts.push(("custom".into(), schedule.clone()));
        }
        let seg = widgets::segmented(&opts, current, move |id| {
            if let Some(e) = w.upgrade() {
                match id.as_str() {
                    "off" => e.set_config("snapshots.schedule", ""),
                    "hourly" => e.set_config("snapshots.schedule", "@hourly"),
                    "daily" => e.set_config("snapshots.schedule", "@daily"),
                    "weekly" => e.set_config("snapshots.schedule", "@weekly"),
                    _ => {}
                }
            }
        });
        self.options.append(&widgets::row("Automatic snapshots", "Taken on a schedule while it runs.", Some(seg.upcast_ref())));
        let entry = |e: &Rc<Editor>, title: &str, desc: &str, key: &'static str, placeholder: &str| {
            let en = gtk::Entry::new();
            en.set_text(&e.config(key));
            en.set_placeholder_text(Some(placeholder));
            en.set_width_chars(24);
            let w = Rc::downgrade(e);
            en.connect_changed(move |en| {
                if let Some(e) = w.upgrade() {
                    e.set_config(key, en.text().trim());
                }
            });
            widgets::row(title, desc, Some(en.upcast_ref()))
        };
        self.options.append(&entry(self, "Keep snapshots for", "Older automatic snapshots are deleted.", "snapshots.expiry", "2w"));
        self.options.append(&section("Shortcuts"));
        self.options.append(&entry(self, "Web UI address", "What its card's Web UI button opens.", "user.nyx.webui", "http://10.0.0.5:8080"));
        let yaml = gtk::Button::with_label("Edit as YAML…");
        let w = Rc::downgrade(self);
        yaml.connect_clicked(move |_| {
            if let Some(e) = w.upgrade() {
                e.open_yaml();
            }
        });
        self.options.append(&widgets::row("Everything else", "The instance's own config and devices, as Incus stores them.", Some(yaml.upcast_ref())));
    }

    fn open_yaml(self: &Rc<Self>) {
        use sourceview5::prelude::*;
        let (dialog, card) = widgets::dialog(&format!("{} as YAML", self.name), 720);
        dialog.set_default_height(600);
        let d = self.draft.borrow();
        let doc = serde_yaml::to_string(&serde_json::json!({ "config": d.config, "devices": d.devices })).unwrap_or_default();
        drop(d);
        let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
        buffer.set_language(sourceview5::LanguageManager::default().language("yaml").as_ref());
        widgets::apply_source_scheme(&buffer);
        buffer.set_text(&doc);
        let view = sourceview5::View::with_buffer(&buffer);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.set_tab_width(2);
        view.set_insert_spaces_instead_of_tabs(true);
        view.add_css_class("compose-editor");
        let scroll = gtk::ScrolledWindow::builder().vexpand(true).hexpand(true).min_content_height(380).child(&view).build();
        scroll.add_css_class("tab-well");
        card.append(&scroll);
        let buttons = widgets::hbox(8);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Not now");
        let ok = gtk::Button::with_label("Use these settings");
        ok.add_css_class("suggested-action");
        buttons.append(&cancel);
        buttons.append(&ok);
        card.append(&buttons);
        let dc = dialog.clone();
        cancel.connect_clicked(move |_| dc.close());
        let (w, dc) = (Rc::downgrade(self), dialog.clone());
        ok.connect_clicked(move |_| {
            let Some(e) = w.upgrade() else { return };
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
            #[derive(serde::Deserialize)]
            struct Doc {
                #[serde(default)]
                config: BTreeMap<String, String>,
                #[serde(default)]
                devices: Devices,
            }
            match serde_yaml::from_str::<Doc>(&text) {
                Ok(doc) => {
                    {
                        let mut d = e.draft.borrow_mut();
                        d.config = doc.config;
                        d.devices = doc.devices;
                    }
                    e.repaint();
                    dc.close();
                    window::toast("Review the changes, then Apply.");
                }
                Err(err) => window::toast(&format!("That isn't valid: {err}")),
            }
        });
        dialog.present();
    }
}

fn sanitize(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect();
    if out.is_empty() { "share".into() } else { out }
}

/// USB devices plugged into this computer: (vendor, product, "Name (vvvv:pppp)").
fn local_usb() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") else { return out };
    for e in entries.flatten() {
        let p = e.path();
        let read = |f: &str| std::fs::read_to_string(p.join(f)).ok().map(|s| s.trim().to_string());
        let (Some(v), Some(pr)) = (read("idVendor"), read("idProduct")) else { continue };
        if read("bDeviceClass").as_deref() == Some("09") {
            continue;
        }
        let name = [read("manufacturer"), read("product")].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let name = if name.is_empty() { "USB device".to_string() } else { name };
        out.push((v.clone(), pr.clone(), format!("{name} ({v}:{pr})")));
    }
    out.sort_by(|a, b| a.2.cmp(&b.2));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

fn section(text: &str) -> gtk::Label {
    let l = widgets::label(&text.to_uppercase(), "hw-section");
    l.set_xalign(0.0);
    l
}

fn row_text(title: &str, desc: &str) -> gtk::Box {
    let b = widgets::vbox(2);
    let t = widgets::label(title, "settings-option-title");
    t.set_xalign(0.0);
    b.append(&t);
    let d = widgets::label(desc, "dim");
    d.set_xalign(0.0);
    d.set_wrap(true);
    b.append(&d);
    b
}

fn clear(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_sets_round_trip() {
        assert_eq!(ranges(&[2, 3, 4, 5]), "2-5");
        assert_eq!(ranges(&[0, 2, 3, 7]), "0,2-3,7");
        assert_eq!(pinned("2-5"), vec![2, 3, 4, 5]);
        assert!(pinned("4").is_empty());
    }
}
