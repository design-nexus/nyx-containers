//! A Docker container's environment, mounts, networks and labels, and the raw
//! `docker inspect` document. Values that look like secrets stay hidden until asked.

use crate::models::Workload;
use crate::{widgets, window, worker};
use gtk::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

struct View {
    body: gtk::Box,
}

thread_local! {
    static VIEWS: RefCell<HashMap<String, View>> = RefCell::new(HashMap::new());
}

pub fn build(w: &Workload) -> gtk::Widget {
    let body = widgets::vbox(12);
    body.add_css_class("tab-pad");
    body.append(&widgets::label("Loading…", "dim"));
    let key = w.key();
    VIEWS.with(|v| v.borrow_mut().insert(key.clone(), View { body: body.clone() }));
    worker::inspect_raw(w.host, w.target_id(), key);
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&body).build();
    scroll.upcast()
}

fn secret(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    ["PASSWORD", "PASSWD", "SECRET", "TOKEN", "KEY", "PRIVATE", "CREDENTIAL"].iter().any(|s| k.contains(s))
}

fn group(title: &str, rows: Vec<gtk::Box>) -> gtk::Box {
    let wrap = widgets::vbox(8);
    let t = widgets::label(&title.to_uppercase(), "hw-section");
    t.set_xalign(0.0);
    t.set_margin_start(2);
    wrap.append(&t);
    let list = widgets::vbox(0);
    list.add_css_class("group-list");
    list.add_css_class("inspect-list");
    if rows.is_empty() {
        list.append(&widgets::row("None", "", None));
    }
    for r in rows {
        list.append(&r);
    }
    wrap.append(&list);
    wrap
}

fn copy_button(text: String) -> gtk::Button {
    let b = widgets::icon_button("edit-copy-symbolic", "Copy");
    b.connect_clicked(move |_| {
        window::copy_text(&text);
        window::toast("Copied.");
    });
    b
}

pub fn apply(key: &str, result: Result<Value, String>) {
    let Some(body) = VIEWS.with(|v| v.borrow().get(key).map(|v| v.body.clone())) else { return };
    while let Some(c) = body.first_child() {
        body.remove(&c);
    }
    let info = match result {
        Ok(v) => v,
        Err(e) => {
            body.append(&widgets::label(&format!("Couldn't inspect it: {e}"), "dim"));
            return;
        }
    };
    let s = |v: &Value| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());

    let mut env_rows = Vec::new();
    for item in info.pointer("/Config/Env").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        let line = s(&item);
        let (k, v) = line.split_once('=').unwrap_or((&line, ""));
        let value = widgets::label(if secret(k) { "••••••••" } else { v }, "mono-path");
        value.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        value.set_hexpand(true);
        value.set_xalign(1.0);
        value.set_selectable(!secret(k));
        let controls = widgets::hbox(4);
        controls.append(&value);
        if secret(k) {
            let reveal = widgets::icon_button("view-reveal-symbolic", "Show the value");
            let (l, real) = (value.clone(), v.to_string());
            reveal.connect_clicked(move |_| {
                l.set_text(&real);
                l.set_selectable(true);
            });
            controls.append(&reveal);
        }
        controls.append(&copy_button(v.to_string()));
        env_rows.push(widgets::row(k, "", Some(controls.upcast_ref())));
    }
    body.append(&group("Environment", env_rows));

    let mut mount_rows = Vec::new();
    for m in info.get("Mounts").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        let dest = m.get("Destination").map(s).unwrap_or_default();
        let source = m.get("Name").filter(|n| !n.is_null()).map(s).unwrap_or_else(|| m.get("Source").map(s).unwrap_or_default());
        let kind = m.get("Type").map(s).unwrap_or_default();
        let rw = m.get("RW").and_then(|v| v.as_bool()).unwrap_or(true);
        let desc = format!("{kind} · {source}{}", if rw { "" } else { " · read-only" });
        mount_rows.push(widgets::row(&dest, &desc, Some(copy_button(source).upcast_ref())));
    }
    body.append(&group("Mounts", mount_rows));

    let mut net_rows = Vec::new();
    if let Some(nets) = info.pointer("/NetworkSettings/Networks").and_then(|v| v.as_object()) {
        for (name, n) in nets {
            let ip = n.get("IPAddress").map(s).filter(|x| !x.is_empty()).unwrap_or_else(|| "no address".into());
            let gw = n.get("Gateway").map(s).filter(|x| !x.is_empty());
            let mac = n.get("MacAddress").map(s).filter(|x| !x.is_empty());
            let desc = [Some(ip.clone()), gw.map(|g| format!("gateway {g}")), mac].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            net_rows.push(widgets::row(name, &desc, Some(copy_button(ip).upcast_ref())));
        }
    }
    body.append(&group("Networks", net_rows));

    let mut label_rows = Vec::new();
    if let Some(labels) = info.pointer("/Config/Labels").and_then(|v| v.as_object()) {
        for (k, v) in labels {
            label_rows.push(widgets::row(k, &s(v), None));
        }
    }
    body.append(&group("Labels", label_rows));

    let raw = serde_json::to_string_pretty(&info).unwrap_or_default();
    let head = widgets::hbox(8);
    let t = widgets::label("RAW", "hw-section");
    t.set_hexpand(true);
    t.set_xalign(0.0);
    head.append(&t);
    head.append(&copy_button(raw.clone()));
    body.append(&head);
    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    buffer.set_text(&raw);
    let view = gtk::TextView::with_buffer(&buffer);
    view.set_editable(false);
    view.set_monospace(true);
    view.add_css_class("logs-view");
    view.set_left_margin(14);
    view.set_top_margin(10);
    view.set_bottom_margin(10);
    let well = gtk::ScrolledWindow::builder().min_content_height(260).max_content_height(420).propagate_natural_height(true).child(&view).build();
    well.add_css_class("tab-well");
    body.append(&well);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_spotted_by_name() {
        assert!(secret("POSTGRES_PASSWORD"));
        assert!(secret("api_token"));
        assert!(!secret("TZ"));
    }
}
