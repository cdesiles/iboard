//! The macOS menu bar.
//!
//! winit installs a default menu named after the process (`roadmap`). This
//! replaces it with one named after the app: `Roadmap Viewer` with About,
//! Hide and Quit, then File and Window menus. Call [`install`] once the event
//! loop is running, from the main thread.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationName, NSAboutPanelOptionApplicationVersion,
    NSAboutPanelOptionCredits, NSAboutPanelOptionVersion, NSApplication, NSEventModifierFlags,
    NSMenu, NSMenuItem,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSString};

/// Matches `CFBundleName` in `macos/Info.plist`, which names the menu bar.
const APP_NAME: &str = "Roadmap Viewer";

define_class!(
    /// Receives the menu actions AppKit has no standard selector for.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RoadmapViewerMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            show_about_panel(MainThreadMarker::from(self));
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// Replaces winit's default menu bar. Does nothing off the main thread.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // Menu items hold their target weakly; the target lives as long as the app.
    let target = Box::leak(Box::new(MenuTarget::new(mtm)));

    let menubar = NSMenu::new(mtm);

    // Application menu.
    let app_menu = submenu(mtm, &menubar, APP_NAME);
    let about = item(
        mtm,
        &format!("About {APP_NAME}"),
        Some(sel!(showAbout:)),
        "",
    );
    unsafe { about.setTarget(Some(target)) };
    app_menu.addItem(&about);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    let services_menu = NSMenu::new(mtm);
    let services = item(mtm, "Services", None, "");
    services.setSubmenu(Some(&services_menu));
    app_menu.addItem(&services);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&item(
        mtm,
        &format!("Hide {APP_NAME}"),
        Some(sel!(hide:)),
        "h",
    ));
    let hide_others = item(mtm, "Hide Others", Some(sel!(hideOtherApplications:)), "h");
    hide_others
        .setKeyEquivalentModifierMask(NSEventModifierFlags::Option | NSEventModifierFlags::Command);
    app_menu.addItem(&hide_others);
    app_menu.addItem(&item(
        mtm,
        "Show All",
        Some(sel!(unhideAllApplications:)),
        "",
    ));
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&item(
        mtm,
        &format!("Quit {APP_NAME}"),
        Some(sel!(terminate:)),
        "q",
    ));

    // File menu.
    let file_menu = submenu(mtm, &menubar, "File");
    file_menu.addItem(&item(mtm, "Close Window", Some(sel!(performClose:)), "w"));

    // Window menu: AppKit appends the open windows to it.
    let window_menu = submenu(mtm, &menubar, "Window");
    window_menu.addItem(&item(mtm, "Minimize", Some(sel!(performMiniaturize:)), "m"));
    window_menu.addItem(&item(mtm, "Zoom", Some(sel!(performZoom:)), ""));
    window_menu.addItem(&NSMenuItem::separatorItem(mtm));
    window_menu.addItem(&item(
        mtm,
        "Bring All to Front",
        Some(sel!(arrangeInFront:)),
        "",
    ));

    app.setServicesMenu(Some(&services_menu));
    app.setWindowsMenu(Some(&window_menu));
    app.setMainMenu(Some(&menubar));
}

/// The standard About panel, with the app name, version and description.
fn show_about_panel(mtm: MainThreadMarker) {
    let name = NSString::from_str(APP_NAME);
    let version = NSString::from_str(env!("CARGO_PKG_VERSION"));
    let empty = NSString::from_str("");
    let credits =
        NSAttributedString::from_nsstring(&NSString::from_str(env!("CARGO_PKG_DESCRIPTION")));
    let options = unsafe {
        NSDictionary::<NSString, AnyObject>::from_slices(
            &[
                NSAboutPanelOptionApplicationName,
                NSAboutPanelOptionApplicationVersion,
                // Without it the panel adds a "(<build>)" suffix.
                NSAboutPanelOptionVersion,
                NSAboutPanelOptionCredits,
            ],
            &[&name, &version, &empty, &credits],
        )
    };
    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.orderFrontStandardAboutPanelWithOptions(&options) };
    app.activate();
}

/// Adds a top-level menu titled `title` to `menubar` and returns it.
fn submenu(mtm: MainThreadMarker, menubar: &NSMenu, title: &str) -> Retained<NSMenu> {
    let title = NSString::from_str(title);
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &title);
    let holder = NSMenuItem::new(mtm);
    holder.setTitle(&title);
    holder.setSubmenu(Some(&menu));
    menubar.addItem(&holder);
    menu
}

/// A menu item sending `action` up the responder chain; `key` is its
/// ⌘-shortcut, or empty for none.
fn item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    key: &str,
) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}
