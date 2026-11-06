//! The macOS traffic lights on the header bar's centre line. AppKit centres
//! them on its own 28pt title bar and puts them back after a resize or a
//! full-screen exit, so every frame checks where they are.

use egui::{Pos2, Vec2};

/// Where a button of `size` centred on `centre`, counted from the window's
/// top left, has its origin in a title bar view `height` tall at the window's
/// top. AppKit's y grows upwards unless the view is `flipped`.
#[cfg(any(target_os = "macos", test))]
fn origin(centre: Pos2, size: Vec2, height: f32, flipped: bool) -> Pos2 {
    let y = if flipped {
        centre.y - size.y / 2.0
    } else {
        height - centre.y - size.y / 2.0
    };
    Pos2::new(centre.x - size.x / 2.0, y)
}

/// Moves the window's buttons back where they belong, and looks again once a
/// resize or a full-screen change has settled, when AppKit lays them out last.
#[cfg(target_os = "macos")]
pub(super) fn keep_placed(ctx: &egui::Context) {
    use objc2::MainThreadMarker;
    use objc2::runtime::NSObjectProtocol;
    use objc2::{ClassType, rc::Retained};
    use objc2_app_kit::{NSApplication, NSPanel, NSView, NSWindowButton, NSWindowStyleMask};

    const SETTLES_IN: std::time::Duration = std::time::Duration::from_millis(300);

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let buttons = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ];
    for window in NSApplication::sharedApplication(mtm).windows() {
        // The open panel and the About panel keep their own lights; in full
        // screen the lights live in the menu bar's strip.
        if window.isKindOfClass(NSPanel::class())
            || window.styleMask().contains(NSWindowStyleMask::FullScreen)
        {
            continue;
        }
        for (kind, centre) in buttons.iter().zip(crate::theme::traffic_light_centres()) {
            let Some(button) = window.standardWindowButton(*kind) else {
                continue;
            };
            // SAFETY: on the main thread, and the button is alive for the call.
            let Some(parent): Option<Retained<NSView>> = (unsafe { button.superview() }) else {
                continue;
            };
            let frame = button.frame();
            let size = Vec2::new(frame.size.width as f32, frame.size.height as f32);
            let height = parent.frame().size.height as f32;
            let at = origin(centre, size, height, parent.isFlipped());
            let (x, y) = (f64::from(at.x), f64::from(at.y));
            if (frame.origin.x - x).abs() > 0.25 || (frame.origin.y - y).abs() > 0.25 {
                button.setFrameOrigin(objc2_foundation::NSPoint::new(x, y));
            }
        }
    }
    let id = egui::Id::new("traffic-lights-window");
    let now = ctx.input(|i| (i.content_rect().size(), i.viewport().fullscreen));
    if ctx.data(|d| d.get_temp(id)) != Some(now) {
        ctx.data_mut(|d| d.insert_temp(id, now));
        ctx.request_repaint_after(SETTLES_IN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    /// The buttons' centres land 20pt under the window's top, at x 20, 40
    /// and 60, whichever way the title bar view counts.
    #[test]
    fn the_lights_centre_on_the_bars_centre_line() {
        let size = Vec2::new(14.0, 16.0);
        let [close, minimize, zoom] = theme::traffic_light_centres();
        assert_eq!(
            [close, minimize, zoom].map(|c| c.x),
            [20.0, 40.0, 60.0],
            "every 20pt"
        );
        assert_eq!(close.y, theme::BAR_H / 2.0);

        let up = origin(close, size, theme::TITLEBAR_H, false);
        assert_eq!(up, Pos2::new(13.0, 0.0), "inside the 28pt title bar");
        assert_eq!(theme::TITLEBAR_H - (up.y + size.y / 2.0), close.y);

        let down = origin(zoom, size, theme::TITLEBAR_H, true);
        assert_eq!(down, Pos2::new(53.0, 12.0));
    }
}
