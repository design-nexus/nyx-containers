use crate::models::{Action, HostId, Stack};
use crate::services::compose;
use crate::widgets;
use crate::{window, worker};
use gtk::gio;
use gtk::prelude::*;
use sourceview5::prelude::*;
use std::path::{Path, PathBuf};

pub fn run_stack(stack: &Stack, action: Action) {
    window::toast(&format!("{} {}…", action.gerund(), stack.name));
    if let Some((dir, file)) = compose_paths(stack)
        && stack.host == HostId::Local
    {
        worker::compose(stack.host, stack.name.clone(), dir, file, action);
        return;
    }
    worker::stack_op(stack.host, stack.names.clone(), action);
}

pub fn compose_paths(stack: &Stack) -> Option<(String, String)> {
    let dir = stack.project_dir.clone().filter(|s| !s.is_empty())?;
    let file = stack.compose_file.clone().unwrap_or_else(|| default_compose_path(Path::new(&dir)).display().to_string());
    Some((dir, file))
}

pub fn default_compose_path(dir: &Path) -> PathBuf {
    for name in ["compose.yaml", "compose.yml", "docker-compose.yml", "docker-compose.yaml"] {
        let p = dir.join(name);
        if p.exists() {
            return p;
        }
    }
    dir.join("compose.yaml")
}

pub fn open_editor(existing: Option<Stack>) {
    let title = existing.as_ref().map(|s| format!("Edit {}", s.name)).unwrap_or_else(|| "New stack".into());
    let (dialog, card) = widgets::dialog(&title, 780);
    dialog.set_default_height(640);

    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Project name"));
    name.set_hexpand(true);
    if let Some(s) = &existing {
        name.set_text(&s.name);
    }
    card.append(&widgets::row("Name", "Compose project name.", Some(name.upcast_ref())));

    let dir = gtk::Entry::new();
    dir.set_placeholder_text(Some("/path/to/project"));
    dir.set_hexpand(true);
    if let Some(s) = &existing
        && let Some(d) = &s.project_dir
    {
        dir.set_text(d);
    }
    let browse = gtk::Button::with_label("Browse");
    let dir_row = widgets::hbox(8);
    dir_row.append(&dir);
    dir_row.append(&browse);
    card.append(&widgets::row("Directory", "YAML is saved here. Deploy runs on this computer.", Some(dir_row.upcast_ref())));

    let d_for_browse = dir.clone();
    let parent = window::window();
    browse.connect_clicked(move |_| {
        let chooser = gtk::FileDialog::new();
        chooser.set_title("Project directory");
        let dest = d_for_browse.clone();
        chooser.select_folder(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
            {
                dest.set_text(&path.display().to_string());
            }
        });
    });

    let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
    buffer.set_language(sourceview5::LanguageManager::default().language("yaml").as_ref());
    widgets::apply_source_scheme(&buffer);
    let yaml_text = existing.as_ref().and_then(load_yaml).unwrap_or_else(|| compose::TEMPLATE.to_string());
    buffer.set_text(&yaml_text);
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_auto_indent(true);
    view.set_tab_width(2);
    view.set_insert_spaces_instead_of_tabs(true);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    view.set_left_margin(8);
    view.set_right_margin(8);
    view.add_css_class("compose-editor");
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(320)
        .child(&view)
        .build();
    card.append(&scroll);

    let error = widgets::label("", "danger");
    error.set_xalign(0.0);
    error.set_wrap(true);
    error.set_visible(false);
    card.append(&error);

    let local_only = existing.as_ref().is_some_and(|s| s.host != HostId::Local);
    if local_only {
        let note = widgets::label("This stack is on the remote host. Save writes a local file; Deploy stays on this computer.", "dim");
        note.set_wrap(true);
        note.set_xalign(0.0);
        card.append(&note);
    }

    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Not now");
    let save = gtk::Button::with_label("Save");
    let deploy = gtk::Button::with_label("Deploy");
    deploy.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&save);
    buttons.append(&deploy);
    card.append(&buttons);

    let d = dialog.clone();
    cancel.connect_clicked(move |_| d.close());

    let save_fn = {
        let name = name.clone();
        let dir = dir.clone();
        let buffer = buffer.clone();
        let error = error.clone();
        let existing = existing.clone();
        move || match write_compose(&name.text(), &dir.text(), &buffer, existing.as_ref()) {
            Ok((project, dir, file)) => {
                error.set_visible(false);
                window::toast(&format!("Saved {file}"));
                Ok((project, dir, file))
            }
            Err(err) => {
                error.set_text(&err);
                error.set_visible(true);
                Err(err)
            }
        }
    };
    let save_click = save_fn.clone();
    save.connect_clicked(move |_| {
        let _ = save_click();
    });
    let d = dialog.clone();
    deploy.connect_clicked(move |_| {
        if let Ok((project, dir, file)) = save_fn() {
            window::toast(&format!("Deploying {project}…"));
            worker::compose(HostId::Local, project, dir, file, Action::Start);
            d.close();
        }
    });
    dialog.present();
}

pub fn load_yaml(stack: &Stack) -> Option<String> {
    let (_, file) = compose_paths(stack)?;
    std::fs::read_to_string(file).ok()
}

pub fn write_compose(
    name: &str,
    dir: &str,
    buffer: &sourceview5::Buffer,
    existing: Option<&Stack>,
) -> Result<(String, String, String), String> {
    let project = name.trim();
    if project.is_empty() {
        return Err("A project name is required.".into());
    }
    let dir = dir.trim();
    if dir.is_empty() {
        return Err("A project directory is required.".into());
    }
    let yaml = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
    compose::validate_yaml(&yaml)?;
    let path = existing
        .and_then(compose_paths)
        .map(|(_, file)| PathBuf::from(file))
        .unwrap_or_else(|| default_compose_path(Path::new(dir)));
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {dir}: {e}"))?;
    crate::cmd::atomic_write(&path, &yaml).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok((project.to_string(), dir.to_string(), path.display().to_string()))
}
