//! The image on the macOS pasteboard, and the `⌘V` that asks for it.
//!
//! winit hands egui a paste only when the pasteboard holds text: with a
//! screenshot alone, `⌘V` reaches egui as nothing at all. A local event
//! monitor notices the key instead, without taking it from winit, and the app
//! reads the flag once per frame. Call [`watch`] once, from the main thread.

use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2_app_kit::{
    NSEvent, NSEventMask, NSEventModifierFlags, NSPasteboard, NSPasteboardTypePNG,
    NSPasteboardTypeString, NSPasteboardTypeTIFF,
};

/// Set by the monitor on `⌘V`, taken by the app's next frame.
static PASTED: AtomicBool = AtomicBool::new(false);

/// The running app's context, so a paste wakes its frame loop.
static CONTEXT: OnceLock<eframe::egui::Context> = OnceLock::new();

/// Starts noticing `⌘V`. Later calls do nothing.
pub fn watch(ctx: eframe::egui::Context) {
    if CONTEXT.set(ctx).is_err() {
        return;
    }
    let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event for the call.
        let key = unsafe { event.as_ref() };
        let command = key.modifierFlags().contains(NSEventModifierFlags::Command);
        let v = key
            .charactersIgnoringModifiers()
            .is_some_and(|chars| chars.to_string().eq_ignore_ascii_case("v"));
        if command && v {
            PASTED.store(true, Ordering::SeqCst);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }
        event.as_ptr()
    });
    // SAFETY: the handler returns the event it was given, a valid pointer.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
    };
    // The monitor lasts as long as the app.
    std::mem::forget(monitor);
}

/// Whether `⌘V` was pressed since the last call.
pub fn take_paste() -> bool {
    PASTED.swap(false, Ordering::SeqCst)
}

/// Whether the pasteboard holds text, which a paste prefers to an image.
pub fn has_text() -> bool {
    // SAFETY: the type is one of AppKit's own constants.
    let text = unsafe { NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeString) };
    text.is_some_and(|text| !text.to_string().is_empty())
}

/// The pasteboard's image as PNG or TIFF bytes, if it holds one.
pub fn image() -> Option<Vec<u8>> {
    let board = NSPasteboard::generalPasteboard();
    // SAFETY: the types are AppKit's own constants.
    let data = unsafe {
        board
            .dataForType(NSPasteboardTypePNG)
            .or_else(|| board.dataForType(NSPasteboardTypeTIFF))
    }?;
    Some(data.to_vec())
}
