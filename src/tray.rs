//! Tray icon, as a StatusNotifierItem with a dbusmenu menu.
//!
//! Waybar, KDE, and GNOME with the AppIndicator extension show these. Both
//! interfaces are served over GDBus on the GTK main loop, so the handlers can
//! use the app directly. The item registers with the tray under the app's
//! unique bus name, so the Flatpak only needs to talk to the watcher.

use std::collections::HashMap;

use gtk::gio::prelude::*;
use gtk::glib::{self, Variant};
use gtk::{gdk_pixbuf, gio};

use crate::theme::RESOURCE_BASE;
use crate::{APP_ID, APP_NAME};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

const INTERFACES: &str = r#"
<node>
  <interface name="org.kde.StatusNotifierItem">
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconPixmap" type="a(iiay)" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
    <property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
    <property name="AttentionMovieName" type="s" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
    <method name="ContextMenu"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Activate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="SecondaryActivate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
    <method name="Scroll"><arg name="delta" type="i" direction="in"/><arg name="orientation" type="s" direction="in"/></method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewAttentionIcon"/>
    <signal name="NewOverlayIcon"/>
    <signal name="NewToolTip"/>
    <signal name="NewStatus"><arg name="status" type="s"/></signal>
  </interface>
  <interface name="com.canonical.dbusmenu">
    <property name="Version" type="u" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
    <method name="GetLayout">
      <arg type="i" name="parentId" direction="in"/>
      <arg type="i" name="recursionDepth" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="u" name="revision" direction="out"/>
      <arg type="(ia{sv}av)" name="layout" direction="out"/>
    </method>
    <method name="GetGroupProperties">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="as" name="propertyNames" direction="in"/>
      <arg type="a(ia{sv})" name="properties" direction="out"/>
    </method>
    <method name="GetProperty">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="name" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <method name="Event">
      <arg type="i" name="id" direction="in"/>
      <arg type="s" name="eventId" direction="in"/>
      <arg type="v" name="data" direction="in"/>
      <arg type="u" name="timestamp" direction="in"/>
    </method>
    <method name="EventGroup">
      <arg type="a(isvu)" name="events" direction="in"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <method name="AboutToShow">
      <arg type="i" name="id" direction="in"/>
      <arg type="b" name="needUpdate" direction="out"/>
    </method>
    <method name="AboutToShowGroup">
      <arg type="ai" name="ids" direction="in"/>
      <arg type="ai" name="updatesNeeded" direction="out"/>
      <arg type="ai" name="idErrors" direction="out"/>
    </method>
    <signal name="ItemsPropertiesUpdated">
      <arg type="a(ia{sv})" name="updatedProps"/>
      <arg type="a(ias)" name="removedProps"/>
    </signal>
    <signal name="LayoutUpdated"><arg type="u" name="revision"/><arg type="i" name="parent"/></signal>
    <signal name="ItemActivationRequested"><arg type="i" name="id"/><arg type="u" name="timestamp"/></signal>
  </interface>
</node>"#;

/// What the user asked for through the tray icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Left click: show the notes, or hide them if some are showing.
    Toggle,
    NewNote,
    ShowAll,
    HideAll,
    Manager,
    Quit,
}

/// The menu: (id, label, action). `None` is a separator.
pub const MENU: [(i32, &str, Option<Action>); 6] = [
    (1, "New Note", Some(Action::NewNote)),
    (2, "Show All Notes", Some(Action::ShowAll)),
    (3, "Hide All Notes", Some(Action::HideAll)),
    (4, "All Notes…", Some(Action::Manager)),
    (5, "", None),
    (6, "Quit", Some(Action::Quit)),
];

/// A registered tray icon. Dropping it removes the icon.
pub struct Tray {
    connection: gio::DBusConnection,
    registrations: Vec<gio::RegistrationId>,
    /// Stops watching for the tray.
    unwatch: Option<Box<dyn FnOnce()>>,
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
        for id in self.registrations.drain(..) {
            let _ = self.connection.unregister_object(id);
        }
    }
}

/// Serve the tray icon on `connection` and register it with the tray whenever
/// one is (or later becomes) available. `on_action` runs on the main loop.
pub fn start(connection: &gio::DBusConnection, on_action: impl Fn(Action) + 'static) -> Result<Tray, glib::Error> {
    let node = gio::DBusNodeInfo::for_xml(INTERFACES)?;
    let item = node.lookup_interface("org.kde.StatusNotifierItem").expect("item interface");
    let menu = node.lookup_interface("com.canonical.dbusmenu").expect("menu interface");
    let on_action = std::rc::Rc::new(on_action);

    let pixmaps = icon_pixmaps();
    let item_registration = connection
        .register_object(ITEM_PATH, &item)
        .property(move |_, _, _, _, name| item_property(name, &pixmaps))
        .method_call({
            let on_action = on_action.clone();
            move |_, _, _, _, method, _, invocation| {
                match method {
                    "Activate" => on_action(Action::Toggle),
                    "SecondaryActivate" => on_action(Action::NewNote),
                    _ => {} // ContextMenu, Scroll: the tray shows our menu itself
                }
                invocation.return_value(None);
            }
        })
        .build()?;
    let menu_registration = connection
        .register_object(MENU_PATH, &menu)
        .property(|_, _, _, _, name| menu_property(name))
        .method_call(move |_, _, _, _, method, params, invocation| {
            let reply = menu_call(method, &params, &|id| {
                if let Some(action) = action_for(id) {
                    on_action(action);
                }
            });
            invocation.return_value(reply.as_ref());
        })
        .build()?;

    let watcher = gio::bus_watch_name_on_connection(
        connection,
        WATCHER,
        gio::BusNameWatcherFlags::NONE,
        |connection, _, _| register(&connection),
        |_, _| {},
    );
    Ok(Tray {
        connection: connection.clone(),
        registrations: vec![item_registration, menu_registration],
        unwatch: Some(Box::new(move || gio::bus_unwatch_name(watcher))),
    })
}

/// Tell the tray's watcher about us. Called again whenever it (re)starts.
fn register(connection: &gio::DBusConnection) {
    let Some(name) = connection.unique_name() else { return };
    connection.call(
        Some(WATCHER),
        "/StatusNotifierWatcher",
        WATCHER,
        "RegisterStatusNotifierItem",
        Some(&(name.as_str(),).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        |result| {
            if let Err(err) = result {
                eprintln!("realer-xpad: could not add the tray icon: {err}");
            }
        },
    );
}

pub fn action_for(id: i32) -> Option<Action> {
    MENU.iter().find(|(i, ..)| *i == id).and_then(|(_, _, action)| *action)
}

type Pixmap = (i32, i32, Vec<u8>);

/// The icon goes out as pixels only. Trays prefer `IconName` when it's set, and
/// some (Qt Quick shells) draw a "missing" checkerboard when their own lookup
/// of an app icon fails, while every tray can show pixels.
fn item_property(name: &str, pixmaps: &[Pixmap]) -> Variant {
    let none: Vec<Pixmap> = Vec::new();
    match name {
        "Category" => "ApplicationStatus".to_variant(),
        "Id" => APP_ID.to_variant(),
        "Title" => APP_NAME.to_variant(),
        "Status" => "Active".to_variant(),
        "WindowId" => 0i32.to_variant(),
        "IconPixmap" => pixmaps.to_vec().to_variant(),
        "ItemIsMenu" => false.to_variant(),
        "Menu" => glib::variant::ObjectPath::try_from(MENU_PATH).expect("object path").to_variant(),
        "ToolTip" => ("", none, APP_NAME, "Click to show or hide your notes").to_variant(),
        "OverlayIconPixmap" | "AttentionIconPixmap" => none.to_variant(),
        _ => "".to_variant(), // IconName, IconThemePath, OverlayIconName, AttentionIconName, AttentionMovieName
    }
}

fn menu_property(name: &str) -> Variant {
    match name {
        "Version" => 3u32.to_variant(),
        "TextDirection" => "ltr".to_variant(),
        "Status" => "normal".to_variant(),
        _ => Vec::<String>::new().to_variant(), // IconThemePath
    }
}

fn item_props(id: i32) -> HashMap<String, Variant> {
    let mut props = HashMap::new();
    if id == 0 {
        props.insert("children-display".into(), "submenu".to_variant());
        return props;
    }
    match MENU.iter().find(|(i, ..)| *i == id) {
        Some((_, _, None)) => {
            props.insert("type".into(), "separator".to_variant());
        }
        Some((_, label, Some(_))) => {
            props.insert("label".into(), label.to_variant());
            props.insert("enabled".into(), true.to_variant());
            props.insert("visible".into(), true.to_variant());
        }
        None => {}
    }
    props
}

/// `(ia{sv}av)`: an item with its properties and, for the root, its children.
fn layout(id: i32) -> Variant {
    let children: Vec<Variant> = if id == 0 { MENU.iter().map(|(i, ..)| layout(*i)).collect() } else { Vec::new() };
    (id, item_props(id), children).to_variant()
}

/// Answer a dbusmenu method call. `clicked` is called with the id of a clicked item.
pub fn menu_call(method: &str, params: &Variant, clicked: &dyn Fn(i32)) -> Option<Variant> {
    match method {
        "GetLayout" => {
            let parent = params.child_value(0).get::<i32>().unwrap_or(0);
            // Build the tuple by hand: a Variant inside a Rust tuple would become "v".
            Some(Variant::tuple_from_iter([1u32.to_variant(), layout(parent)]))
        }
        "GetGroupProperties" => {
            let ids = params.child_value(0).get::<Vec<i32>>().unwrap_or_default();
            let props: Vec<(i32, HashMap<String, Variant>)> = ids.into_iter().map(|id| (id, item_props(id))).collect();
            Some((props,).to_variant())
        }
        "GetProperty" => {
            let (id, name) = params.get::<(i32, String)>().unwrap_or_default();
            let value = item_props(id).remove(&name).unwrap_or_else(|| "".to_variant());
            Some((value,).to_variant())
        }
        "Event" => {
            let id = params.child_value(0).get::<i32>().unwrap_or(-1);
            if params.child_value(1).str() == Some("clicked") {
                clicked(id);
            }
            None
        }
        "EventGroup" => {
            for event in params.child_value(0).iter() {
                if event.child_value(1).str() == Some("clicked")
                    && let Some(id) = event.child_value(0).get::<i32>()
                {
                    clicked(id);
                }
            }
            Some((Vec::<i32>::new(),).to_variant())
        }
        "AboutToShow" => Some((false,).to_variant()),
        "AboutToShowGroup" => Some((Vec::<i32>::new(), Vec::<i32>::new()).to_variant()),
        _ => None,
    }
}

/// The logo at a few sizes, as ARGB32 in network byte order.
pub fn icon_pixmaps() -> Vec<Pixmap> {
    let path = format!("{RESOURCE_BASE}/icons/scalable/apps/{APP_ID}.svg");
    [16, 22, 24, 32, 48, 64]
        .into_iter()
        .filter_map(|size| {
            let pixbuf = gdk_pixbuf::Pixbuf::from_resource_at_scale(&path, size, size, true).ok()?;
            Some((pixbuf.width(), pixbuf.height(), argb(&pixbuf)))
        })
        .collect()
}

fn argb(pixbuf: &gdk_pixbuf::Pixbuf) -> Vec<u8> {
    let (w, h, stride) = (pixbuf.width() as usize, pixbuf.height() as usize, pixbuf.rowstride() as usize);
    let channels = pixbuf.n_channels() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let p = &bytes[y * stride + x * channels..];
            let alpha = if channels == 4 { p[3] } else { 255 };
            out.extend_from_slice(&[alpha, p[0], p[1], p[2]]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_lists_the_menu() {
        let reply = menu_call("GetLayout", &(0i32, -1i32, Vec::<String>::new()).to_variant(), &|_| {}).unwrap();
        assert_eq!(reply.type_().as_str(), "(u(ia{sv}av))");
        let root = reply.child_value(1);
        assert_eq!(root.child_value(0).get::<i32>(), Some(0));
        let children = root.child_value(2);
        assert_eq!(children.n_children(), MENU.len());
        let first = children.child_value(0).as_variant().unwrap();
        let props = first.child_value(1).get::<HashMap<String, Variant>>().unwrap();
        assert_eq!(props["label"].str(), Some("New Note"));
        let separator = children.child_value(4).as_variant().unwrap();
        let props = separator.child_value(1).get::<HashMap<String, Variant>>().unwrap();
        assert_eq!(props["type"].str(), Some("separator"));
    }

    #[test]
    fn clicks_map_to_actions() {
        let clicked = std::cell::Cell::new(None);
        let event = (6i32, "clicked", Variant::from_variant(&"".to_variant()), 0u32).to_variant();
        assert!(menu_call("Event", &event, &|id| clicked.set(Some(id))).is_none());
        assert_eq!(clicked.get().and_then(action_for), Some(Action::Quit));
        let hovered = (1i32, "hovered", Variant::from_variant(&"".to_variant()), 0u32).to_variant();
        clicked.set(None);
        menu_call("Event", &hovered, &|id| clicked.set(Some(id)));
        assert_eq!(clicked.get(), None, "only clicks act");
        assert_eq!(action_for(5), None, "the separator does nothing");
    }

    #[test]
    fn group_properties_and_about_to_show() {
        let reply = menu_call("GetGroupProperties", &(vec![1i32, 6], Vec::<String>::new()).to_variant(), &|_| {});
        assert_eq!(reply.unwrap().type_().as_str(), "(a(ia{sv}))");
        let reply = menu_call("AboutToShow", &(0i32,).to_variant(), &|_| {}).unwrap();
        assert_eq!(reply.get::<(bool,)>(), Some((false,)));
    }
}
