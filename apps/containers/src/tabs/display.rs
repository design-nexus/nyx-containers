//! A VM's screen inside its tab, over SPICE. Incus serves the VM's VGA console as a
//! websocket; the worker bridges it to a private Unix socket and spice-client-glib
//! speaks SPICE over that. Frames are copied into a texture once per frame; mouse
//! and keyboard go back as SPICE input. A click captures the keyboard (system
//! shortcuts included, as in VMware); Ctrl+Alt releases it.

use crate::models::{Action, HostId, RunState, Workload};
use crate::sections::actions;
use crate::{widgets, window, worker};
use gtk::glib::translate::{FromGlibPtrFull, ToGlibPtr};
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

mod ffi {
    use gtk::glib::gobject_ffi::GObject;
    use std::ffi::c_void;

    #[repr(C)]
    pub struct Primary {
        pub format: i32,
        pub width: i32,
        pub height: i32,
        pub stride: i32,
        pub shmid: i32,
        pub data: *mut u8,
        pub marked: i32,
    }

    #[link(name = "spice-client-glib-2.0")]
    unsafe extern "C" {
        pub fn spice_session_new() -> *mut GObject;
        pub fn spice_session_connect(session: *mut GObject) -> i32;
        pub fn spice_session_disconnect(session: *mut GObject);
        pub fn spice_channel_connect(channel: *mut GObject) -> i32;
        pub fn spice_display_channel_get_primary(channel: *mut GObject, surface: u32, primary: *mut Primary) -> i32;
        pub fn spice_inputs_channel_motion(channel: *mut GObject, dx: i32, dy: i32, buttons: i32);
        pub fn spice_inputs_channel_position(channel: *mut GObject, x: i32, y: i32, display: i32, buttons: i32);
        pub fn spice_inputs_channel_button_press(channel: *mut GObject, button: i32, buttons: i32);
        pub fn spice_inputs_channel_button_release(channel: *mut GObject, button: i32, buttons: i32);
        pub fn spice_inputs_channel_key_press(channel: *mut GObject, scancode: u32);
        pub fn spice_inputs_channel_key_release(channel: *mut GObject, scancode: u32);
        pub fn spice_main_channel_update_display(channel: *mut GObject, id: i32, x: i32, y: i32, w: i32, h: i32, update: i32);
        pub fn spice_main_channel_update_display_enabled(channel: *mut GObject, id: i32, enabled: i32, update: i32);
        pub fn spice_audio_get(session: *mut GObject, context: *mut c_void) -> *mut GObject;
    }
}

const MOUSE_MODE_CLIENT: i32 = 2;
const CHANNEL_CLOSED: i32 = 12;

pub struct Display {
    pub root: gtk::Box,
    seq: Cell<u64>,
    workload: Workload,
    stage: gtk::Stack,
    message_title: gtk::Label,
    message_body: gtk::Label,
    message_action: gtk::Button,
    scroller: gtk::ScrolledWindow,
    picture: gtk::Picture,
    status: gtk::Label,
    fit_button: gtk::ToggleButton,
    resize_button: gtk::ToggleButton,
    fs_bar: gtk::Revealer,
    session: RefCell<Option<glib::Object>>,
    main: RefCell<Option<glib::Object>>,
    inputs: RefCell<Option<glib::Object>>,
    display: RefCell<Option<glib::Object>>,
    dirty: Cell<bool>,
    damage: Cell<Option<(i32, i32, i32, i32)>>,
    full_frame: Cell<bool>,
    guest: Cell<(i32, i32)>,
    buttons: Cell<i32>,
    last: Cell<(f64, f64)>,
    held: RefCell<HashSet<u32>>,
    asked: Cell<(i32, i32)>,
    stable: Cell<(i32, i32, i64)>,
    fullscreen: Cell<bool>,
    bar_hold: Cell<u32>,
    captured: Cell<bool>,
    texture: RefCell<Option<gdk::Texture>>,
    closed: Cell<bool>,
}

thread_local! {
    static DISPLAYS: RefCell<HashMap<u64, Weak<Display>>> = RefCell::new(HashMap::new());
}

fn raw(o: &glib::Object) -> *mut glib::gobject_ffi::GObject {
    o.to_glib_none().0
}

fn has_signal(o: &glib::Object, name: &str) -> bool {
    glib::subclass::signal::SignalId::lookup(name, o.type_()).is_some()
}

impl Display {
    pub fn new(w: &Workload) -> Rc<Display> {
        let root = widgets::vbox(0);
        root.add_css_class("display-view");

        let bar = widgets::hbox(4);
        bar.add_css_class("tab-subbar");
        bar.add_css_class("display-bar");
        let seg = widgets::hbox(0);
        seg.add_css_class("segmented");
        let fit_button = gtk::ToggleButton::new();
        fit_button.set_icon_name("zoom-fit-best-symbolic");
        fit_button.add_css_class("segment");
        fit_button.set_tooltip_text(Some("Fit the window"));
        let actual = gtk::ToggleButton::new();
        actual.set_icon_name("zoom-original-symbolic");
        actual.add_css_class("segment");
        actual.set_tooltip_text(Some("Actual size"));
        actual.set_group(Some(&fit_button));
        fit_button.set_active(true);
        seg.append(&fit_button);
        seg.append(&actual);
        bar.append(&seg);
        let resize_button = gtk::ToggleButton::new();
        resize_button.set_icon_name("view-fullscreen-symbolic");
        resize_button.add_css_class("flat");
        resize_button.set_tooltip_text(Some("Resize the guest to fit (needs the SPICE guest agent)"));
        resize_button.set_active(true);
        bar.append(&resize_button);
        let keys = widgets::icon_button("input-keyboard-symbolic", "Send keys");
        bar.append(&keys);
        let paste = widgets::icon_button("edit-paste-symbolic", "Type the clipboard into the guest");
        bar.append(&paste);
        let shot = widgets::icon_button("camera-photo-symbolic", "Save a screenshot to Pictures");
        bar.append(&shot);
        let full = widgets::icon_button("view-fullscreen-symbolic", "Fullscreen (Ctrl+Alt+Enter)");
        bar.append(&full);
        let viewer = widgets::icon_button("window-new-symbolic", "Open in a separate viewer window");
        bar.append(&viewer);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        bar.append(&spacer);
        let status = widgets::label("", "display-status");
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        bar.append(&status);
        root.append(&bar);

        let picture = gtk::Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Contain);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_focusable(true);
        picture.set_can_focus(true);
        picture.add_css_class("grabs-keys");
        picture.add_css_class("display-picture");
        picture.set_tooltip_text(None);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&picture)
            .hexpand(true)
            .vexpand(true)
            .build();
        let well = gtk::Overlay::new();
        well.add_css_class("display-well");
        well.set_child(Some(&scroller));
        let fs_bar = gtk::Revealer::new();
        fs_bar.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        fs_bar.set_halign(gtk::Align::Center);
        fs_bar.set_valign(gtk::Align::Start);
        well.add_overlay(&fs_bar);

        let stage = gtk::Stack::new();
        stage.set_vexpand(true);
        let message = widgets::vbox(8);
        message.add_css_class("empty-state");
        message.set_valign(gtk::Align::Center);
        message.set_halign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name("video-display-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("dim");
        message.append(&icon);
        let message_title = widgets::label("", "empty-title");
        message.append(&message_title);
        let message_body = widgets::label("", "dim");
        message_body.set_wrap(true);
        message_body.set_max_width_chars(56);
        message_body.set_justify(gtk::Justification::Center);
        message.append(&message_body);
        let message_action = gtk::Button::with_label("Start");
        message_action.add_css_class("suggested-action");
        message_action.set_halign(gtk::Align::Center);
        message_action.set_margin_top(8);
        message.append(&message_action);
        stage.add_named(&message, Some("message"));
        stage.add_named(&well, Some("screen"));
        let stage_wrap = widgets::vbox(0);
        stage_wrap.add_css_class("tab-pad");
        stage_wrap.append(&stage);
        root.append(&stage_wrap);

        let d = Rc::new(Display {
            root,
            seq: Cell::new(0),
            workload: w.clone(),
            stage,
            message_title,
            message_body,
            message_action,
            scroller,
            picture,
            status,
            fit_button: fit_button.clone(),
            resize_button: resize_button.clone(),
            fs_bar,
            session: RefCell::new(None),
            main: RefCell::new(None),
            inputs: RefCell::new(None),
            display: RefCell::new(None),
            dirty: Cell::new(false),
            damage: Cell::new(None),
            full_frame: Cell::new(true),
            guest: Cell::new((0, 0)),
            buttons: Cell::new(0),
            last: Cell::new((0.0, 0.0)),
            held: RefCell::new(HashSet::new()),
            asked: Cell::new((0, 0)),
            stable: Cell::new((0, 0, 0)),
            fullscreen: Cell::new(false),
            bar_hold: Cell::new(0),
            captured: Cell::new(false),
            texture: RefCell::new(None),
            closed: Cell::new(false),
        });
        d.build_fs_bar();
        d.wire_controls(&fit_button, &keys, &paste, &shot, &full, &viewer);
        d.wire_input();
        let weak = Rc::downgrade(&d);
        // On the root, not the picture: the picture isn't mapped until the first frame arrives.
        d.root.add_tick_callback(move |_, clock| {
            match weak.upgrade() {
                Some(d) => {
                    d.tick(clock.frame_time());
                    glib::ControlFlow::Continue
                }
                None => glib::ControlFlow::Break,
            }
        });
        let message_weak = Rc::downgrade(&d);
        d.message_action.connect_clicked(move |_| {
            if let Some(d) = message_weak.upgrade() {
                if d.workload.state == RunState::Running {
                    d.start();
                } else {
                    actions::run(&d.workload, Action::Start);
                    d.show_message("Starting…", "The screen appears once the VM is up.", None);
                    let weak = Rc::downgrade(&d);
                    glib::timeout_add_local_once(std::time::Duration::from_secs(4), move || {
                        if let Some(d) = weak.upgrade() {
                            d.start();
                        }
                    });
                }
            }
        });
        if w.state == RunState::Running {
            d.start();
        } else {
            d.show_message(&format!("{} is off", w.name), "Start it to see its screen here.", Some("Start"));
        }
        d
    }

    fn show_message(&self, title: &str, body: &str, action: Option<&str>) {
        self.message_title.set_text(title);
        self.message_body.set_text(body);
        self.message_action.set_visible(action.is_some());
        if let Some(a) = action {
            self.message_action.set_label(a);
        }
        self.stage.set_visible_child_name("message");
    }

    fn start(self: &Rc<Self>) {
        self.disconnect();
        self.closed.set(false);
        let seq = crate::sections::session::next_seq();
        self.seq.set(seq);
        DISPLAYS.with(|m| m.borrow_mut().insert(seq, Rc::downgrade(self)));
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let socket = dir.join("nyx-containers").join(format!("display-{}-{seq}.sock", std::process::id()));
        self.status.set_text("Connecting…");
        self.show_message("Connecting…", &format!("Opening {}'s screen.", self.workload.name), None);
        worker::start_vga(self.workload.host, self.workload.target_id(), seq, socket);
    }

    fn connect_spice(self: &Rc<Self>, socket: &std::path::Path) {
        // SAFETY: spice_session_new returns a new reference that `from_glib_full` takes over.
        let session: glib::Object = unsafe { glib::Object::from_glib_full(ffi::spice_session_new()) };
        session.set_property("unix-path", socket.to_string_lossy().to_string());
        if session.find_property("enable-usbredir").is_some() {
            session.set_property("enable-usbredir", false);
        }
        let weak = Rc::downgrade(self);
        session.connect_local("channel-new", false, move |args| {
            let channel = args.get(1).and_then(|v| v.get::<glib::Object>().ok())?;
            if let Some(d) = weak.upgrade() {
                d.channel_new(&channel);
            }
            None
        });
        // SAFETY: the session outlives the call; the returned audio object is owned by it.
        unsafe {
            ffi::spice_audio_get(raw(&session), std::ptr::null_mut());
            ffi::spice_session_connect(raw(&session));
        }
        *self.session.borrow_mut() = Some(session);
    }

    fn channel_new(self: &Rc<Self>, channel: &glib::Object) {
        let kind = channel.type_().name().to_string();
        let weak = Rc::downgrade(self);
        match kind.as_str() {
            "SpiceMainChannel" => {
                *self.main.borrow_mut() = Some(channel.clone());
                let w = weak.clone();
                channel.connect_notify_local(Some("agent-connected"), move |_, _| {
                    if let Some(d) = w.upgrade() {
                        d.paint_status();
                        d.asked.set((0, 0));
                    }
                });
                let w = weak.clone();
                channel.connect_notify_local(Some("mouse-mode"), move |_, _| {
                    if let Some(d) = w.upgrade() {
                        d.paint_status();
                    }
                });
                if has_signal(channel, "channel-event") {
                    channel.connect_local("channel-event", false, move |args| {
                        let event = args.get(1).map(|v| unsafe { glib::gobject_ffi::g_value_get_enum(v.to_glib_none().0) }).unwrap_or(0);
                        if let Some(d) = weak.upgrade()
                            && event >= CHANNEL_CLOSED
                            && !d.closed.get()
                        {
                            d.lost(if event == CHANNEL_CLOSED { "The display closed." } else { "The display connection failed." });
                        }
                        None
                    });
                }
            }
            "SpiceDisplayChannel" => {
                *self.display.borrow_mut() = Some(channel.clone());
                for sig in ["display-primary-create", "display-mark"] {
                    if has_signal(channel, sig) {
                        let w = weak.clone();
                        channel.connect_local(sig, false, move |_| {
                            if let Some(d) = w.upgrade() {
                                d.dirty.set(true);
                                d.damage.set(None);
                                d.full_frame.set(true);
                            }
                            None
                        });
                    }
                }
                if has_signal(channel, "display-invalidate") {
                    let w = weak.clone();
                    channel.connect_local("display-invalidate", false, move |args| {
                        let int = |i: usize| args.get(i).and_then(|v| v.get::<i32>().ok()).unwrap_or(0);
                        if let Some(d) = w.upgrade() {
                            let (x, y, ww, hh) = (int(1), int(2), int(3), int(4));
                            // Grow the changed area to cover this one too.
                            let next = match d.damage.get() {
                                Some((ax, ay, aw, ah)) => {
                                    let (x0, y0) = (ax.min(x), ay.min(y));
                                    let (x1, y1) = ((ax + aw).max(x + ww), (ay + ah).max(y + hh));
                                    (x0, y0, x1 - x0, y1 - y0)
                                }
                                None => (x, y, ww, hh),
                            };
                            d.damage.set(Some(next));
                            d.dirty.set(true);
                        }
                        None
                    });
                }
                if has_signal(channel, "display-primary-destroy") {
                    let w = weak.clone();
                    channel.connect_local("display-primary-destroy", false, move |_| {
                        if let Some(d) = w.upgrade() {
                            d.guest.set((0, 0));
                        }
                        None
                    });
                }
                // SAFETY: the channel is alive; connecting it starts its stream.
                unsafe { ffi::spice_channel_connect(raw(channel)) };
            }
            "SpiceInputsChannel" => {
                *self.inputs.borrow_mut() = Some(channel.clone());
                unsafe { ffi::spice_channel_connect(raw(channel)) };
            }
            "SpiceCursorChannel" => {
                if has_signal(channel, "cursor-set") {
                    let w = weak.clone();
                    channel.connect_local("cursor-set", false, move |args| {
                        let int = |i: usize| args.get(i).and_then(|v| v.get::<i32>().ok()).unwrap_or(0);
                        let (width, height, hx, hy) = (int(1), int(2), int(3), int(4));
                        let data = args.get(5).map(|v| unsafe { glib::gobject_ffi::g_value_get_pointer(v.to_glib_none().0) });
                        if let (Some(d), Some(ptr)) = (w.upgrade(), data)
                            && !ptr.is_null()
                            && width > 0
                            && height > 0
                        {
                            // SAFETY: SPICE hands over width × height RGBA pixels for this call.
                            let pixels = unsafe { std::slice::from_raw_parts(ptr as *const u8, (width * height * 4) as usize) };
                            let bytes = glib::Bytes::from(pixels);
                            let texture = gdk::MemoryTexture::new(width, height, gdk::MemoryFormat::R8g8b8a8, &bytes, (width * 4) as usize);
                            let cursor = gdk::Cursor::from_texture(&texture, hx, hy, None);
                            d.picture.set_cursor(Some(&cursor));
                        }
                        None
                    });
                }
                if has_signal(channel, "cursor-hide") {
                    let w = weak.clone();
                    channel.connect_local("cursor-hide", false, move |_| {
                        if let Some(d) = w.upgrade() {
                            d.picture.set_cursor_from_name(Some("none"));
                        }
                        None
                    });
                }
                if has_signal(channel, "cursor-reset") {
                    let w = weak.clone();
                    channel.connect_local("cursor-reset", false, move |_| {
                        if let Some(d) = w.upgrade() {
                            d.picture.set_cursor_from_name(Some("default"));
                        }
                        None
                    });
                }
                unsafe { ffi::spice_channel_connect(raw(channel)) };
            }
            _ => {}
        }
    }

    fn tick(&self, now: i64) {
        if self.dirty.replace(false)
            && let Some(ch) = self.display.borrow().as_ref()
        {
            let mut p = ffi::Primary { format: 0, width: 0, height: 0, stride: 0, shmid: 0, data: std::ptr::null_mut(), marked: 0 };
            // SAFETY: `p` is a valid out-parameter; the data stays valid until we return to the main loop.
            let ok = unsafe { ffi::spice_display_channel_get_primary(raw(ch), 0, &mut p) } != 0;
            if ok && !p.data.is_null() && p.width > 0 && p.height > 0 {
                let len = (p.stride * p.height) as usize;
                let pixels = unsafe { std::slice::from_raw_parts(p.data, len) };
                let bytes = glib::Bytes::from(pixels);
                let builder = gdk::MemoryTextureBuilder::new()
                    .set_bytes(Some(&bytes))
                    .set_width(p.width)
                    .set_height(p.height)
                    .set_stride(p.stride as usize)
                    .set_format(gdk::MemoryFormat::B8g8r8x8);
                // Only the changed rectangle goes to the GPU when the size holds.
                let previous = self.texture.borrow().clone().filter(|t| t.width() == p.width && t.height() == p.height);
                let builder = match (previous, self.damage.take(), self.full_frame.replace(false)) {
                    (Some(prev), Some((x, y, w, h)), false) => builder
                        .set_update_texture(Some(&prev))
                        .set_update_region(Some(&gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(x, y, w, h)))),
                    _ => builder,
                };
                let texture = builder.build();
                self.picture.set_paintable(Some(&texture));
                *self.texture.borrow_mut() = Some(texture);
                if self.guest.get() != (p.width, p.height) {
                    self.guest.set((p.width, p.height));
                    self.paint_status();
                }
                if self.stage.visible_child_name().as_deref() != Some("screen") {
                    self.stage.set_visible_child_name("screen");
                }
            }
        }
        self.follow_size(now);
    }

    /// Ask the guest for a resolution that fills the view once the size settles.
    fn follow_size(&self, now: i64) {
        if !self.resize_button.is_active() || !self.fit_button.is_active() || !self.agent() {
            return;
        }
        let Some(main) = self.main.borrow().clone() else { return };
        let scale = self.picture.scale_factor().max(1);
        let (w, h) = (self.scroller.width() * scale, self.scroller.height() * scale);
        if w < 320 || h < 240 {
            return;
        }
        let (w, h) = (w & !7, h & !1);
        let (sw, sh, since) = self.stable.get();
        if (sw, sh) != (w, h) {
            self.stable.set((w, h, now));
            return;
        }
        if now - since < 500_000 || self.asked.get() == (w, h) || self.guest.get() == (w, h) {
            return;
        }
        self.asked.set((w, h));
        // SAFETY: the main channel is alive while the session is.
        unsafe {
            ffi::spice_main_channel_update_display_enabled(raw(&main), 0, 1, 0);
            ffi::spice_main_channel_update_display(raw(&main), 0, 0, 0, w, h, 1);
        }
    }

    fn agent(&self) -> bool {
        self.main.borrow().as_ref().is_some_and(|m| m.find_property("agent-connected").is_some() && m.property::<bool>("agent-connected"))
    }

    fn client_mouse(&self) -> bool {
        self.main.borrow().as_ref().is_some_and(|m| m.find_property("mouse-mode").is_some() && m.property::<i32>("mouse-mode") == MOUSE_MODE_CLIENT)
    }

    fn paint_status(&self) {
        let (w, h) = self.guest.get();
        let mut parts = vec!["Running".to_string()];
        if let Some(ip) = &self.workload.ip {
            parts.push(ip.clone());
        }
        if w > 0 {
            parts.push(format!("{w} × {h}"));
        }
        parts.push(if self.agent() { "Guest agent connected".into() } else { "No guest agent".into() });
        if self.captured.get() {
            parts.push("Ctrl+Alt releases".into());
        } else {
            parts.push("Click to capture".into());
        }
        self.status.set_text(&parts.join(" · "));
        self.resize_button.set_sensitive(self.agent());
    }

    fn lost(self: &Rc<Self>, why: &str) {
        self.disconnect();
        self.show_message(why, "The VM may have shut down or restarted.", Some("Reconnect"));
        self.status.set_text("Disconnected");
    }

    fn disconnect(&self) {
        self.release_all();
        if let Some(s) = self.session.borrow_mut().take() {
            // SAFETY: the session is alive until we drop our reference below.
            unsafe { ffi::spice_session_disconnect(raw(&s)) };
        }
        self.main.borrow_mut().take();
        self.inputs.borrow_mut().take();
        self.display.borrow_mut().take();
        let seq = self.seq.get();
        if seq != 0 {
            worker::stop_term(seq);
            DISPLAYS.with(|m| m.borrow_mut().remove(&seq));
        }
    }

    pub fn close(&self) {
        self.closed.set(true);
        if self.fullscreen.get() {
            window::set_display_fullscreen(false);
        }
        self.disconnect();
    }

    // ---------- Input ----------

    fn guest_point(&self, x: f64, y: f64) -> Option<(i32, i32)> {
        let (gw, gh) = self.guest.get();
        if gw <= 0 || gh <= 0 {
            return None;
        }
        let (pw, ph) = (self.picture.width() as f64, self.picture.height() as f64);
        let (gx, gy) = if self.fit_button.is_active() {
            let scale = (pw / gw as f64).min(ph / gh as f64);
            if scale <= 0.0 {
                return None;
            }
            let (ox, oy) = ((pw - gw as f64 * scale) / 2.0, (ph - gh as f64 * scale) / 2.0);
            ((x - ox) / scale, (y - oy) / scale)
        } else {
            let (ox, oy) = (((pw - gw as f64) / 2.0).max(0.0), ((ph - gh as f64) / 2.0).max(0.0));
            (x - ox, y - oy)
        };
        Some((gx.clamp(0.0, gw as f64 - 1.0) as i32, gy.clamp(0.0, gh as f64 - 1.0) as i32))
    }

    fn send_motion(&self, x: f64, y: f64) {
        let Some(inputs) = self.inputs.borrow().clone() else { return };
        let (lx, ly) = self.last.replace((x, y));
        unsafe {
            if self.client_mouse() {
                if let Some((gx, gy)) = self.guest_point(x, y) {
                    ffi::spice_inputs_channel_position(raw(&inputs), gx, gy, 0, self.buttons.get());
                }
            } else {
                ffi::spice_inputs_channel_motion(raw(&inputs), (x - lx) as i32, (y - ly) as i32, self.buttons.get());
            }
        }
    }

    fn send_button(&self, gdk_button: u32, press: bool) {
        let Some(inputs) = self.inputs.borrow().clone() else { return };
        let (button, mask) = match gdk_button {
            1 => (1, 1),
            2 => (2, 2),
            3 => (3, 4),
            8 => (6, 32),
            9 => (7, 64),
            _ => return,
        };
        let state = if press { self.buttons.get() | mask } else { self.buttons.get() & !mask };
        self.buttons.set(state);
        unsafe {
            if press {
                ffi::spice_inputs_channel_button_press(raw(&inputs), button, state);
            } else {
                ffi::spice_inputs_channel_button_release(raw(&inputs), button, state);
            }
        }
    }

    fn key(&self, scancode: u32, press: bool) {
        let Some(inputs) = self.inputs.borrow().clone() else { return };
        unsafe {
            if press {
                self.held.borrow_mut().insert(scancode);
                ffi::spice_inputs_channel_key_press(raw(&inputs), scancode);
            } else {
                self.held.borrow_mut().remove(&scancode);
                ffi::spice_inputs_channel_key_release(raw(&inputs), scancode);
            }
        }
    }

    fn release_all(&self) {
        let held: Vec<u32> = self.held.borrow().iter().copied().collect();
        for sc in held {
            self.key(sc, false);
        }
        for b in [1, 2, 3] {
            if self.buttons.get() != 0 {
                self.send_button(b, false);
            }
        }
    }

    fn send_combo(&self, codes: &[u32]) {
        for c in codes {
            self.key(*c, true);
        }
        for c in codes.iter().rev() {
            self.key(*c, false);
        }
    }

    fn set_captured(&self, on: bool) {
        // Only a window the user is typing in may take the keyboard from the desktop.
        let active = self.picture.root().and_downcast::<gtk::Window>().is_some_and(|w| w.is_active());
        let on = on && active;
        if self.captured.replace(on) == on {
            return;
        }
        if let Some(toplevel) = self.picture.native().and_then(|n| n.surface()).and_downcast::<gdk::Toplevel>() {
            if on {
                toplevel.inhibit_system_shortcuts(None::<&gdk::Event>);
            } else {
                toplevel.restore_system_shortcuts();
            }
        }
        if !on {
            self.release_all();
        }
        self.paint_status();
    }

    fn wire_input(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            if let Some(d) = weak.upgrade() {
                d.send_motion(x, y);
            }
        });
        self.picture.add_controller(motion);

        let click = gtk::GestureClick::new();
        click.set_button(0);
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |g, _, x, y| {
            let Some(d) = weak.upgrade() else { return };
            if !d.picture.has_focus() {
                d.picture.grab_focus();
            }
            d.set_captured(true);
            d.send_motion(x, y);
            d.send_button(g.current_button(), true);
        });
        let weak = Rc::downgrade(self);
        click.connect_released(move |g, _, x, y| {
            let Some(d) = weak.upgrade() else { return };
            d.send_motion(x, y);
            d.send_button(g.current_button(), false);
        });
        self.picture.add_controller(click);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE);
        let weak = Rc::downgrade(self);
        scroll.connect_scroll(move |_, _, dy| {
            let Some(d) = weak.upgrade() else { return glib::Propagation::Proceed };
            let Some(inputs) = d.inputs.borrow().clone() else { return glib::Propagation::Stop };
            let (button, mask) = if dy < 0.0 { (4, 8) } else { (5, 16) };
            unsafe {
                ffi::spice_inputs_channel_button_press(raw(&inputs), button, d.buttons.get() | mask);
                ffi::spice_inputs_channel_button_release(raw(&inputs), button, d.buttons.get());
            }
            glib::Propagation::Stop
        });
        self.picture.add_controller(scroll);

        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |c, keyval, keycode, state| {
            let Some(d) = weak.upgrade() else { return glib::Propagation::Proceed };
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let alt = state.contains(gdk::ModifierType::ALT_MASK);
            let modifier_key = matches!(keyval, gdk::Key::Control_L | gdk::Key::Control_R | gdk::Key::Alt_L | gdk::Key::Alt_R);
            // Ctrl+Alt: give the keyboard and mouse back.
            if modifier_key && ((ctrl && matches!(keyval, gdk::Key::Alt_L | gdk::Key::Alt_R)) || (alt && matches!(keyval, gdk::Key::Control_L | gdk::Key::Control_R))) {
                d.release_all();
                d.set_captured(false);
                if let Some(w) = c.widget().and_then(|w| w.root()).and_downcast::<gtk::Window>() {
                    gtk::prelude::GtkWindowExt::set_focus(&w, None::<&gtk::Widget>);
                }
                d.show_bar_for(4000);
                return glib::Propagation::Stop;
            }
            if ctrl && alt && matches!(keyval, gdk::Key::Return | gdk::Key::KP_Enter) {
                d.release_all();
                d.toggle_fullscreen();
                return glib::Propagation::Stop;
            }
            if let Some(sc) = xt_scancode(keycode.saturating_sub(8)) {
                d.key(sc, true);
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        keys.connect_key_released(move |_, _, keycode, _| {
            if let Some(d) = weak.upgrade()
                && let Some(sc) = xt_scancode(keycode.saturating_sub(8))
                && d.held.borrow().contains(&sc)
            {
                d.key(sc, false);
            }
        });
        self.picture.add_controller(keys);

        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_enter(move |_| {
            if let Some(d) = weak.upgrade() {
                d.set_captured(true);
            }
        });
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            if let Some(d) = weak.upgrade() {
                d.set_captured(false);
            }
        });
        self.picture.add_controller(focus);
    }

    fn wire_controls(
        self: &Rc<Self>,
        fit: &gtk::ToggleButton,
        keys: &gtk::Button,
        paste: &gtk::Button,
        shot: &gtk::Button,
        full: &gtk::Button,
        viewer: &gtk::Button,
    ) {
        let weak = Rc::downgrade(self);
        fit.connect_toggled(move |b| {
            let Some(d) = weak.upgrade() else { return };
            if b.is_active() {
                d.picture.set_can_shrink(true);
                d.picture.set_content_fit(gtk::ContentFit::Contain);
                d.scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
            } else {
                d.picture.set_can_shrink(false);
                d.picture.set_content_fit(gtk::ContentFit::Fill);
                d.scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
            }
        });
        let weak = Rc::downgrade(self);
        keys.connect_clicked(move |b| {
            let Some(d) = weak.upgrade() else { return };
            let send = |codes: &'static [u32]| {
                let w = Rc::downgrade(&d);
                move || {
                    if let Some(d) = w.upgrade() {
                        d.send_combo(codes);
                    }
                }
            };
            let mut entries = vec![
                nyx_ui::menu::item("Ctrl+Alt+Delete", send(&[0x1d, 0x38, 0x153])),
                nyx_ui::menu::item("Ctrl+Alt+Backspace", send(&[0x1d, 0x38, 0x0e])),
                nyx_ui::menu::separator(),
            ];
            const F: [u32; 7] = [0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41];
            for (i, code) in F.iter().enumerate() {
                let codes: &'static [u32] = Box::leak(vec![0x1d, 0x38, *code].into_boxed_slice());
                entries.push(nyx_ui::menu::item(&format!("Ctrl+Alt+F{}", i + 1), send(codes)));
            }
            entries.push(nyx_ui::menu::separator());
            entries.push(nyx_ui::menu::item("Print Screen", send(&[0x137])));
            entries.push(nyx_ui::menu::item("Super (Windows key)", send(&[0x15b])));
            entries.push(nyx_ui::menu::item("Escape", send(&[0x01])));
            nyx_ui::menu::popup_below(b, entries);
        });
        let weak = Rc::downgrade(self);
        paste.connect_clicked(move |_| {
            let Some(d) = weak.upgrade() else { return };
            let Some(display) = gdk::Display::default() else { return };
            let w = Rc::downgrade(&d);
            display.clipboard().read_text_async(None::<&gtk::gio::Cancellable>, move |r| {
                if let (Some(d), Ok(Some(text))) = (w.upgrade(), r) {
                    d.type_text(text.to_string());
                }
            });
        });
        let weak = Rc::downgrade(self);
        shot.connect_clicked(move |_| {
            let Some(d) = weak.upgrade() else { return };
            let Some(texture) = d.texture.borrow().clone() else {
                window::toast("Nothing on the screen yet.");
                return;
            };
            let dir = glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir);
            let stamp = glib::DateTime::now_local().ok().and_then(|t| t.format("%Y-%m-%d %H.%M.%S").ok()).map(|s| s.to_string()).unwrap_or_default();
            let path = dir.join(format!("{} {stamp}.png", d.workload.name));
            match texture.save_to_png(&path) {
                Ok(()) => window::toast(&format!("Saved {}", path.display())),
                Err(e) => window::toast(&format!("Couldn't save the screenshot: {e}")),
            }
        });
        let weak = Rc::downgrade(self);
        full.connect_clicked(move |_| {
            if let Some(d) = weak.upgrade() {
                d.toggle_fullscreen();
            }
        });
        let (host, name) = (self.workload.host, self.workload.name.clone());
        viewer.connect_clicked(move |_| open_viewer(host, &name));
    }

    /// Type `text` into the guest as key presses (US layout), a few keys per frame.
    fn type_text(self: &Rc<Self>, text: String) {
        let keys: Vec<(u32, bool)> = text.chars().filter_map(us_key).collect();
        if keys.is_empty() {
            return;
        }
        window::toast(&format!("Typing {} characters…", keys.len()));
        let weak = Rc::downgrade(self);
        let mut i = 0;
        glib::timeout_add_local(std::time::Duration::from_millis(12), move || {
            let Some(d) = weak.upgrade() else { return glib::ControlFlow::Break };
            let Some((sc, shift)) = keys.get(i).copied() else { return glib::ControlFlow::Break };
            if shift {
                d.key(0x2a, true);
            }
            d.key(sc, true);
            d.key(sc, false);
            if shift {
                d.key(0x2a, false);
            }
            i += 1;
            glib::ControlFlow::Continue
        });
    }

    fn build_fs_bar(self: &Rc<Self>) {
        let bar = widgets::hbox(2);
        bar.add_css_class("display-fs-bar");
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("svc-dot");
        dot.add_css_class("run");
        dot.set_valign(gtk::Align::Center);
        bar.append(&dot);
        let name = widgets::label(&self.workload.name, "display-fs-name");
        bar.append(&name);
        let w = Rc::downgrade(self);
        let keys = widgets::icon_button("input-keyboard-symbolic", "Send Ctrl+Alt+Delete");
        keys.connect_clicked(move |_| {
            if let Some(d) = w.upgrade() {
                d.send_combo(&[0x1d, 0x38, 0x153]);
            }
        });
        bar.append(&keys);
        let w = Rc::downgrade(self);
        let leave = gtk::Button::new();
        let leave_box = widgets::hbox(6);
        leave_box.append(&gtk::Image::from_icon_name("view-restore-symbolic"));
        leave_box.append(&widgets::label("Exit fullscreen", ""));
        leave.set_child(Some(&leave_box));
        leave.add_css_class("display-fs-exit");
        leave.set_tooltip_text(Some("Ctrl+Alt+Enter, or Esc once the keyboard is released"));
        leave.connect_clicked(move |_| {
            if let Some(d) = w.upgrade() {
                d.toggle_fullscreen();
            }
        });
        bar.append(&leave);
        let w = self.workload.clone();
        bar.append(&actions::power_button(&w));
        self.fs_bar.set_child(Some(&bar));
        // Shows when the pointer comes near the top while fullscreen. The zone is deep
        // because the desktop's own bar may take the screen's top few pixels.
        let motion = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        motion.connect_motion(move |_, _, y| {
            if let Some(d) = weak.upgrade()
                && d.fullscreen.get()
            {
                if y < 80.0 {
                    d.fs_bar.set_reveal_child(true);
                } else if y > 150.0 && d.bar_hold.get() == 0 {
                    d.fs_bar.set_reveal_child(false);
                }
            }
        });
        self.picture.add_controller(motion);
    }

    /// Keep the fullscreen bar up for `ms`, then hide it unless the pointer is near it.
    fn show_bar_for(self: &Rc<Self>, ms: u64) {
        if !self.fullscreen.get() {
            return;
        }
        self.fs_bar.set_reveal_child(true);
        let token = self.bar_hold.get().wrapping_add(1);
        self.bar_hold.set(token);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
            if let Some(d) = weak.upgrade()
                && d.bar_hold.get() == token
            {
                d.bar_hold.set(0);
                let near = d.last.get().1 < 80.0;
                if !near {
                    d.fs_bar.set_reveal_child(false);
                }
            }
        });
    }

    /// Leave fullscreen from outside the display (Esc, F11, the window manager).
    pub fn leave_fullscreen(self: &Rc<Self>) {
        if self.fullscreen.get() {
            self.toggle_fullscreen();
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen.get()
    }

    fn toggle_fullscreen(self: &Rc<Self>) {
        let on = !self.fullscreen.get();
        self.fullscreen.set(on);
        window::set_display_fullscreen(on);
        if let Some(bar) = self.root.first_child() {
            bar.set_visible(!on);
        }
        if on {
            self.root.add_css_class("fullscreen");
        } else {
            self.root.remove_css_class("fullscreen");
        }
        if on {
            self.show_bar_for(3000);
            window::toast("Ctrl+Alt releases the keyboard · then Esc leaves fullscreen");
        } else {
            self.fs_bar.set_reveal_child(false);
        }
        self.asked.set((0, 0));
    }
}

/// The display that's fullscreen, if any.
pub fn fullscreen_display() -> Option<Rc<Display>> {
    DISPLAYS.with(|m| m.borrow().values().filter_map(Weak::upgrade).find(|d| d.is_fullscreen()))
}

pub fn apply_ready(seq: u64, result: Result<std::path::PathBuf, String>) {
    let Some(d) = DISPLAYS.with(|m| m.borrow().get(&seq).and_then(Weak::upgrade)) else { return };
    match result {
        Ok(path) => d.connect_spice(&path),
        Err(e) => {
            d.show_message("Couldn't open the display", &e, Some("Try again"));
            d.status.set_text("");
        }
    }
}

pub fn apply_ended(seq: u64, result: Result<(), String>) {
    let Some(d) = DISPLAYS.with(|m| m.borrow().get(&seq).and_then(Weak::upgrade)) else { return };
    if let Err(e) = result
        && !d.closed.get()
    {
        d.show_message("The display stopped", &e, Some("Reconnect"));
    }
}

pub fn open_viewer(host: HostId, name: &str) {
    if host != HostId::Local {
        window::toast("Remote displays open here in the tab.");
        return;
    }
    match std::process::Command::new("incus").args(["console", name, "--type=vga"]).spawn() {
        Ok(_) => window::toast(&format!("Opening {name} in a viewer…")),
        Err(e) => window::toast(&format!("Couldn't start the viewer: {e}")),
    }
}

/// A Linux evdev key code as a PC XT (set 1) scancode, E0-prefixed keys as 0x100 | code.
fn xt_scancode(evdev: u32) -> Option<u32> {
    Some(match evdev {
        1..=83 => evdev,
        86 => 0x56,
        87 => 0x57,
        88 => 0x58,
        96 => 0x11c,
        97 => 0x11d,
        98 => 0x135,
        99 => 0x137,
        100 => 0x138,
        102 => 0x147,
        103 => 0x148,
        104 => 0x149,
        105 => 0x14b,
        106 => 0x14d,
        107 => 0x14f,
        108 => 0x150,
        109 => 0x151,
        110 => 0x152,
        111 => 0x153,
        113 => 0x120,
        114 => 0x12e,
        115 => 0x130,
        119 => 0x145,
        125 => 0x15b,
        126 => 0x15c,
        127 => 0x15d,
        _ => return None,
    })
}

/// The scancode and Shift state that types `c` on a US keyboard.
fn us_key(c: char) -> Option<(u32, bool)> {
    const ROW: &[(char, char, u32)] = &[
        ('1', '!', 0x02), ('2', '@', 0x03), ('3', '#', 0x04), ('4', '$', 0x05), ('5', '%', 0x06), ('6', '^', 0x07),
        ('7', '&', 0x08), ('8', '*', 0x09), ('9', '(', 0x0a), ('0', ')', 0x0b), ('-', '_', 0x0c), ('=', '+', 0x0d),
        ('[', '{', 0x1a), (']', '}', 0x1b), (';', ':', 0x27), ('\'', '"', 0x28), ('`', '~', 0x29), ('\\', '|', 0x2b),
        (',', '<', 0x33), ('.', '>', 0x34), ('/', '?', 0x35),
    ];
    const LETTERS: &str = "qwertyuiop";
    const HOME: &str = "asdfghjkl";
    const BOTTOM: &str = "zxcvbnm";
    match c {
        ' ' => return Some((0x39, false)),
        '\n' => return Some((0x1c, false)),
        '\t' => return Some((0x0f, false)),
        _ => {}
    }
    for (plain, shifted, code) in ROW {
        if c == *plain {
            return Some((*code, false));
        }
        if c == *shifted {
            return Some((*code, true));
        }
    }
    let lower = c.to_ascii_lowercase();
    let shift = c.is_ascii_uppercase();
    for (row, start) in [(LETTERS, 0x10), (HOME, 0x1e), (BOTTOM, 0x2c)] {
        if let Some(i) = row.find(lower) {
            return Some((start + i as u32, shift));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancodes_cover_letters_and_extended_keys() {
        assert_eq!(xt_scancode(30), Some(0x1e));
        assert_eq!(xt_scancode(111), Some(0x153));
        assert_eq!(xt_scancode(125), Some(0x15b));
        assert_eq!(xt_scancode(240), None);
    }

    #[test]
    fn typing_uses_shift_for_capitals_and_symbols() {
        assert_eq!(us_key('a'), Some((0x1e, false)));
        assert_eq!(us_key('A'), Some((0x1e, true)));
        assert_eq!(us_key('!'), Some((0x02, true)));
        assert_eq!(us_key('m'), Some((0x32, false)));
        assert_eq!(us_key('é'), None);
    }
}
