//! The macOS menu bar.
//!
//! winit installs a default menu named after the process (`iboard`). This
//! replaces it with one owned by the app: About, Hide and Quit, then File,
//! Tools and Window menus, the latter also sizing the text through egui's
//! zoom. Call [`install`] once the event loop is running, from the main thread.
//!
//! The Quit item deliberately does *not* send AppKit's `terminate:`. That tears
//! the `NSWindow` down, and the Touch Bar finder then races the teardown and
//! aborts the process on Touch Bar Macs (a double `removeObserver` in
//! `_NSTouchBarFinderObservation`). Instead it sets a flag the app reads each
//! frame and exits from Rust, leaving the window in place. Both binaries install
//! the menu, so neither can regress into the AppKit path.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionApplicationName,
    NSAboutPanelOptionApplicationVersion, NSAboutPanelOptionCredits, NSAboutPanelOptionVersion,
    NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSAttributedStringAttachmentConveniences, NSColor, NSEventModifierFlags, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSImage, NSLinkAttributeName, NSMenu,
    NSMenuItem, NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSStackView,
    NSTextAlignment, NSTextAttachment, NSTextField,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSData, NSDictionary,
    NSMutableAttributedString, NSRange, NSSize, NSString, NSURL,
};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the Quit menu item and read once per frame by the app, so quitting
/// goes through the app (save prompt, hard exit) instead of AppKit's
/// `terminate:`, which tears the window down and can abort on Touch Bar Macs.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by Window › Settings… and read once per frame by the app.
static SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Set by Tools › Project Stats… and read once per frame by the app.
static STATS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The running app's context, so a menu action can wake its frame loop.
static CONTEXT: OnceLock<eframe::egui::Context> = OnceLock::new();

/// The app name and About-panel credits, remembered at [`install`] time so the
/// menu target's `showAbout:` can reach them.
static ABOUT: OnceLock<AboutInfo> = OnceLock::new();

struct AboutInfo {
    name: String,
    credits: String,
    board: bool,
}

/// The Logo's app icon and wordmarks, for the board's About panel.
const ICON: &[u8] = include_bytes!("../design/components/Logo/files/iboard-1024.png");
const WORDMARK_LIGHT: &[u8] =
    include_bytes!("../design/components/Logo/files/iboard-wordmark-light.svg");
const WORDMARK_DARK: &[u8] =
    include_bytes!("../design/components/Logo/files/iboard-wordmark-dark.svg");
/// The wordmark's height in the About panel, and its SVG's aspect.
const WORDMARK_H: f64 = 36.0;
const WORDMARK_ASPECT: f64 = 155.0 / 48.0;

/// Remembers the context for menu actions. Call once, from [`install`]'s caller.
pub fn set_context(ctx: eframe::egui::Context) {
    let _ = CONTEXT.set(ctx);
}

/// Whether the Quit menu item was picked since the last call.
pub fn take_quit_request() -> bool {
    QUIT_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Whether Window › Settings… was picked since the last call.
pub fn take_settings_request() -> bool {
    SETTINGS_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Whether Tools › Project Stats… was picked since the last call.
pub fn take_stats_request() -> bool {
    STATS_REQUESTED.swap(false, Ordering::SeqCst)
}

define_class!(
    /// Receives the menu actions AppKit has no standard selector for.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IboardMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            show_about_panel(MainThreadMarker::from(self));
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            SETTINGS_REQUESTED.store(true, Ordering::SeqCst);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }

        #[unsafe(method(openStats:))]
        fn open_stats(&self, _sender: Option<&AnyObject>) {
            STATS_REQUESTED.store(true, Ordering::SeqCst);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }

        #[unsafe(method(zoomIn:))]
        fn zoom_in(&self, _sender: Option<&AnyObject>) {
            if let Some(ctx) = CONTEXT.get() {
                eframe::egui::gui_zoom::zoom_in(ctx);
            }
        }

        #[unsafe(method(zoomOut:))]
        fn zoom_out(&self, _sender: Option<&AnyObject>) {
            if let Some(ctx) = CONTEXT.get() {
                eframe::egui::gui_zoom::zoom_out(ctx);
            }
        }

        #[unsafe(method(resetZoom:))]
        fn reset_zoom(&self, _sender: Option<&AnyObject>) {
            if let Some(ctx) = CONTEXT.get() {
                ctx.set_zoom_factor(1.0);
            }
        }

        #[unsafe(method(quitIboard:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            QUIT_REQUESTED.store(true, Ordering::SeqCst);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `NSObject`'s designated initializer, on a fresh allocation.
        unsafe { msg_send![super(this), init] }
    }
}

/// Sends `item`'s action to the app's menu target.
fn set_target(item: &NSMenuItem, target: &'static AnyObject) {
    // SAFETY: the target is leaked, so it outlives the item holding it weakly.
    unsafe { item.setTarget(Some(target)) };
}

/// Replaces winit's default menu bar. Does nothing off the main thread.
///
/// `app_name` titles the application menu and the About panel; `credits` is the
/// text under the version there, `**bold**` spans and `https://` links styled. `board` adds Window › Settings… and Tools ›
/// Project Stats…, which the caller watches with [`take_settings_request`] and
/// [`take_stats_request`].
pub fn install(app_name: &str, credits: &str, board: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let _ = ABOUT.set(AboutInfo {
        name: app_name.to_string(),
        credits: credits.to_string(),
        board,
    });
    let app = NSApplication::sharedApplication(mtm);
    // Menu items hold their target weakly; the target lives as long as the app.
    let target: &'static AnyObject = Box::leak(Box::new(MenuTarget::new(mtm)));

    let menubar = NSMenu::new(mtm);

    // Application menu.
    let app_menu = submenu(mtm, &menubar, app_name);
    let about = item(
        mtm,
        &format!("About {app_name}"),
        Some(sel!(showAbout:)),
        "",
    );
    set_target(&about, target);
    app_menu.addItem(&about);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    let services_menu = NSMenu::new(mtm);
    let services = item(mtm, "Services", None, "");
    services.setSubmenu(Some(&services_menu));
    app_menu.addItem(&services);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&item(
        mtm,
        &format!("Hide {app_name}"),
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
    let quit = item(
        mtm,
        &format!("Quit {app_name}"),
        Some(sel!(quitIboard:)),
        "q",
    );
    set_target(&quit, target);
    app_menu.addItem(&quit);

    // File menu.
    let file_menu = submenu(mtm, &menubar, "File");
    file_menu.addItem(&item(mtm, "Close Window", Some(sel!(performClose:)), "w"));

    if board {
        let tools_menu = submenu(mtm, &menubar, "Tools");
        let stats = item(mtm, "Project Stats…", Some(sel!(openStats:)), "");
        set_target(&stats, target);
        tools_menu.addItem(&stats);
    }

    // Window menu: AppKit appends the open windows to it.
    let window_menu = submenu(mtm, &menubar, "Window");
    window_menu.addItem(&item(mtm, "Minimize", Some(sel!(performMiniaturize:)), "m"));
    window_menu.addItem(&item(mtm, "Zoom", Some(sel!(performZoom:)), ""));
    window_menu.addItem(&NSMenuItem::separatorItem(mtm));
    for (title, action, key) in [
        ("Increase Font Size", sel!(zoomIn:), "+"),
        ("Decrease Font Size", sel!(zoomOut:), "-"),
        ("Actual Size", sel!(resetZoom:), "0"),
    ] {
        let zoom = item(mtm, title, Some(action), key);
        set_target(&zoom, target);
        window_menu.addItem(&zoom);
    }
    if board {
        window_menu.addItem(&NSMenuItem::separatorItem(mtm));
        // No key equivalent: `⌘,` stays the app's own toggle, handled in egui.
        let settings_item = item(mtm, "Settings…", Some(sel!(openSettings:)), "");
        set_target(&settings_item, target);
        window_menu.addItem(&settings_item);
    }
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
/// The board's shows the Logo instead: its sharp icon, then the wordmark in
/// place of the name, the version under it.
fn show_about_panel(mtm: MainThreadMarker) {
    let about = ABOUT.get();
    let board = about.is_some_and(|a| a.board);
    let app = NSApplication::sharedApplication(mtm);
    let mut name = NSString::from_str(about.map(|a| a.name.as_str()).unwrap_or("iboard"));
    let mut version = NSString::from_str(env!("CARGO_PKG_VERSION"));
    let empty = NSString::from_str("");
    let credits = NSMutableAttributedString::new();
    let icon = board.then(|| image(ICON)).flatten();
    if board && let Some(mark) = wordmark(&app) {
        credits.appendAttributedString(&mark);
        credits.appendAttributedString(&attributed(
            &format!("\nVersion {version}\n\n"),
            &NSColor::secondaryLabelColor(),
        ));
        name = empty.clone();
        version = empty.clone();
    }
    credits.appendAttributedString(&attributed(
        about.map(|a| a.credits.as_str()).unwrap_or_default(),
        &NSColor::labelColor(),
    ));
    // SAFETY: AppKit's own option keys, each paired with the type it documents.
    let options = unsafe {
        let mut keys = vec![
            NSAboutPanelOptionApplicationName,
            NSAboutPanelOptionApplicationVersion,
            // Without it the panel adds a "(<build>)" suffix.
            NSAboutPanelOptionVersion,
            NSAboutPanelOptionCredits,
        ];
        let mut values: Vec<&AnyObject> = vec![&name, &version, &empty, &credits];
        if let Some(icon) = &icon {
            keys.push(NSAboutPanelOptionApplicationIcon);
            values.push(icon);
        }
        NSDictionary::<NSString, AnyObject>::from_slices(&keys, &values)
    };
    // SAFETY: `options` only holds the About-panel keys and value types above.
    unsafe { app.orderFrontStandardAboutPanelWithOptions(&options) };
    if board {
        collapse_empty_rows(&app);
    }
}

/// An image from PNG or SVG bytes.
fn image(bytes: &[u8]) -> Option<Retained<NSImage>> {
    NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(bytes))
}

/// The wordmark as a centred line of credits, light or dark after the app.
fn wordmark(app: &NSApplication) -> Option<Retained<NSAttributedString>> {
    // SAFETY: AppKit's own appearance names.
    let names = unsafe { NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]) };
    let dark = app
        .effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&names)
        .is_some_and(|name| name.to_string().contains("Dark"));
    let mark = image(if dark { WORDMARK_DARK } else { WORDMARK_LIGHT })?;
    mark.setSize(NSSize::new(WORDMARK_H * WORDMARK_ASPECT, WORDMARK_H));
    let attachment = NSTextAttachment::new();
    attachment.setImage(Some(&mark));
    let line = NSMutableAttributedString::from_attributed_nsstring(
        &NSAttributedString::attributedStringWithAttachment(&attachment),
    );
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setAlignment(NSTextAlignment::Center);
    // SAFETY: a paragraph style under its own key.
    unsafe {
        line.addAttribute_value_range(
            NSParagraphStyleAttributeName,
            &paragraph,
            NSRange::new(0, line.length()),
        );
    }
    Some(Retained::into_super(line))
}

/// Hides the About panel's emptied name and version rows; its stack view
/// then closes the gap they leave above the wordmark.
fn collapse_empty_rows(app: &NSApplication) {
    for window in app.windows() {
        let Some(content) = window.contentView() else {
            continue;
        };
        let Some(stack) = content.subviews().firstObject() else {
            continue;
        };
        let Some(stack) = stack.downcast_ref::<NSStackView>() else {
            continue;
        };
        for row in stack.arrangedSubviews() {
            if let Some(field) = row.downcast_ref::<NSTextField>()
                && field.stringValue().length() == 0
            {
                field.setHidden(true);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum Span<'a> {
    Text(&'a str),
    Bold(&'a str),
    Link(&'a str),
}

/// Splits `text` into plain runs, `**bold**` runs and `https://` links.
fn spans(text: &str) -> Vec<Span<'_>> {
    let mut out = Vec::new();
    for (i, part) in text.split("**").enumerate() {
        if part.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            out.push(Span::Bold(part));
            continue;
        }
        let mut rest = part;
        while let Some(start) = rest.find("https://") {
            let end = rest[start..]
                .find(char::is_whitespace)
                .map_or(rest.len(), |len| start + len);
            if start > 0 {
                out.push(Span::Text(&rest[..start]));
            }
            out.push(Span::Link(&rest[start..end]));
            rest = &rest[end..];
        }
        if !rest.is_empty() {
            out.push(Span::Text(rest));
        }
    }
    out
}

/// The About panel's credits: centred, in the small system font and `color`,
/// with [`spans`] styled.
fn attributed(text: &str, color: &NSColor) -> Retained<NSAttributedString> {
    let size = NSFont::smallSystemFontSize();
    let regular = NSFont::systemFontOfSize(size);
    let bold = NSFont::boldSystemFontOfSize(size);
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setAlignment(NSTextAlignment::Center);
    let out = NSMutableAttributedString::new();
    for span in spans(text) {
        let (run, font, url) = match span {
            Span::Text(run) => (run, &regular, None),
            Span::Bold(run) => (run, &bold, None),
            Span::Link(run) => (
                run,
                &regular,
                NSURL::URLWithString(&NSString::from_str(run)),
            ),
        };
        // SAFETY: AppKit's attribute keys, each paired with the type it documents.
        let attributes = unsafe {
            let mut keys: Vec<&NSAttributedStringKey> = vec![
                NSFontAttributeName,
                NSForegroundColorAttributeName,
                NSParagraphStyleAttributeName,
            ];
            let mut values: Vec<&AnyObject> =
                vec![font.as_ref(), color.as_ref(), paragraph.as_ref()];
            if let Some(url) = &url {
                keys.push(NSLinkAttributeName);
                values.push(url.as_ref());
            }
            NSDictionary::<NSAttributedStringKey, AnyObject>::from_slices(&keys, &values)
        };
        // SAFETY: the attributes above are all valid for an attributed string.
        let piece = unsafe {
            NSAttributedString::initWithString_attributes(
                NSAttributedString::alloc(),
                &NSString::from_str(run),
                Some(&attributes),
            )
        };
        out.appendAttributedString(&piece);
    }
    Retained::into_super(out)
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
    // SAFETY: every `action` passed here is implemented by AppKit's responder
    // chain or by `MenuTarget`.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Span, spans};

    #[test]
    fn credits_style_bold_runs_and_links() {
        assert_eq!(
            spans("**iboard** is a tracker.\nIssues: https://example.com/x"),
            vec![
                Span::Bold("iboard"),
                Span::Text(" is a tracker.\nIssues: "),
                Span::Link("https://example.com/x"),
            ]
        );
        assert_eq!(spans("Plain text"), vec![Span::Text("Plain text")]);
    }
}
