//! Containers on the control interface (`nyx_core::control`, API 2), for `nyx containers …`,
//! Glimpse's Containers panel and Nyx Assistant: this computer's containers and machines
//! (Docker and Incus), started, stopped and restarted, and Containers opened at one.
//! Remote hosts, deleting and everything else stay in the window.

use crate::engines::{docker, incus};
use crate::models::{Action, Engine, HostId, RunState, Workload};
use crate::prefs::RemoteProfile;
use gtk::glib;
use gtk::prelude::*;
use nyx_actions::{Error, ErrorKind};
use nyx_core::control::Answer;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::sync::OnceLock;

pub fn description(app_id: &str) -> Value {
    let name = json!({"type": "string", "minLength": 1, "description": "a container or machine on this computer"});
    let op = |verb: &str, title: &str, text: &str| {
        json!({"name": format!("containers.{verb}"), "description": text,
            "inputSchema": {"type": "object", "properties": {"name": name}, "required": ["name"], "additionalProperties": false},
            "annotations": {"title": title},
            "x-nyx": {"risk": "writes", "positional": ["name"], "suggest": {"name": "containers.list"}, "result": "message"}})
    };
    json!({
        "tools": [
            {"name": "containers.list", "description": "Containers and machines on this computer (Docker and Incus), running ones first; a prefix keeps those whose name starts with it.",
             "inputSchema": {"type": "object", "properties": {"prefix": {"type": "string"}, "limit": {"type": "integer", "minimum": 1}}, "additionalProperties": false},
             "annotations": {"title": "Containers", "readOnlyHint": true},
             "x-nyx": {"risk": "read_only", "aliases": ["ps"], "positional": ["prefix"], "result": "list"}},
            op("start", "Start", "Start a container or machine."),
            op("stop", "Stop", "Stop a container or machine."),
            op("restart", "Restart", "Restart a container or machine."),
            {"name": "containers.open", "description": "Open Containers, at a container or machine when named.",
             "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "additionalProperties": false},
             "annotations": {"title": "Open Containers"},
             "x-nyx": {"risk": "writes", "confirmation": "never", "positional": ["name"], "suggest": {"name": "containers.list"}, "result": "none"}}
        ],
        "nyx": {"api": 2, "app": app_id, "name": "Containers", "icon": crate::APP_ID, "namespaces": ["containers"], "aliases": ["docker"],
                "bus_name": app_id, "activation": "dbus", "default": "containers.list", "open": {"action": "containers.open"},
                "topics": [{"name": "workloads"}],
                "panel": {"id": "containers", "topics": ["workloads"], "size": {"min": [460, 340], "preferred": [540, 420]}}}
    })
}

thread_local! {
    static SERVICE: RefCell<Option<nyx_core::control::Service>> = const { RefCell::new(None) };
    static TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

/// Register the interface (from `connect_startup`); `workloads` is read again every
/// 5 s only while something watches it, and after each change.
pub fn serve(app: &gtk::Application) {
    let id = app.application_id().map(|s| s.to_string()).unwrap_or_else(|| crate::APP_ID.to_string());
    let window_app = app.clone();
    let Some(service) = nyx_core::control::serve_with(app, description(&id), move |request, answer| {
        handle(&window_app, &request.action, &request.arguments, answer)
    }) else {
        return;
    };
    service.on_watch(|_, on| {
        TIMER.with(|t| {
            if let Some(id) = t.borrow_mut().take() {
                id.remove();
            }
            if on {
                publish();
                *t.borrow_mut() = Some(glib::timeout_add_seconds_local(5, || {
                    publish();
                    glib::ControlFlow::Continue
                }));
            }
        });
    });
    SERVICE.with(|s| *s.borrow_mut() = Some(service));
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().expect("containers control"))
}

/// This computer's workloads, Docker's and Incus's.
fn local() -> Vec<Workload> {
    runtime().block_on(async {
        let (d, i) = tokio::join!(docker::probe_local(), incus::probe_local());
        d.1.into_iter().chain(i.1).filter(|w| w.host == HostId::Local).collect()
    })
}

fn state(s: RunState) -> &'static str {
    match s {
        RunState::Running => "running",
        RunState::Paused => "paused",
        RunState::Stopped => "stopped",
        RunState::Other => "other",
    }
}

fn items(mut list: Vec<Workload>, prefix: &str, limit: usize) -> Value {
    let prefix = prefix.to_lowercase();
    list.retain(|w| w.name.to_lowercase().starts_with(&prefix));
    list.sort_by_key(|w| (w.state != RunState::Running, w.name.to_lowercase()));
    let items: Vec<Value> = list
        .iter()
        .take(limit)
        .map(|w| {
            let what = w.image.clone().unwrap_or_else(|| w.status.clone());
            json!({"title": w.name, "value": w.name, "id": w.name, "subtitle": format!("{} · {what}", w.engine.label()),
                   "engine": w.engine.label(), "state": state(w.state), "running": w.state == RunState::Running, "project": w.project})
        })
        .collect();
    json!({ "items": items })
}

fn publish() {
    let Some(service) = SERVICE.with(|s| s.borrow().clone()) else { return };
    if !service.watched("workloads") {
        return;
    }
    nyx_core::cmd::background(local, move |list| service.publish("workloads", items(list, "", 200)));
}

fn failed(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Failed, message)
}

fn handle(app: &gtk::Application, action: &str, args: &Value, answer: Answer) {
    let name = args["name"].as_str().unwrap_or("").trim().to_string();
    let op = match action {
        "containers.list" => {
            let prefix = args["prefix"].as_str().unwrap_or("").to_string();
            let limit = args["limit"].as_u64().unwrap_or(200) as usize;
            return nyx_core::cmd::background(local, move |list| answer(Ok(items(list, &prefix, limit))));
        }
        "containers.open" => {
            crate::window::present(app, None);
            if !name.is_empty() {
                crate::window::open_by_name(&name, None);
            }
            return answer(Ok(Value::Null));
        }
        "containers.start" => Action::Start,
        "containers.stop" => Action::Stop,
        "containers.restart" => Action::Restart,
        _ => return answer(Err(Error::new(ErrorKind::UnknownAction, format!("no action {action}")))),
    };
    nyx_core::cmd::background(
        move || -> Result<String, String> {
            let found = local().into_iter().find(|w| w.name == name).ok_or_else(|| format!("No container or machine called “{name}” on this computer"))?;
            let remote = RemoteProfile::default();
            runtime().block_on(async {
                match found.engine {
                    Engine::Docker => docker::operate(HostId::Local, &remote, &found.name, op).await,
                    Engine::Incus => incus::operate(HostId::Local, &remote, &found.name, op).await,
                }
            })
        },
        move |done| {
            answer(done.map(|message| json!({ "message": message })).map_err(failed));
            publish();
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_description_is_api_2() {
        let d = nyx_actions::Description::from_value(&description(crate::APP_ID), None).unwrap();
        assert_eq!(d.actions.len(), 5);
        assert!(d.actions.iter().all(|a| !a.asks()), "nothing here deletes");
        assert_eq!(d.app.panel.as_ref().map(|p| p.id.as_str()), Some("containers"));
    }
}
