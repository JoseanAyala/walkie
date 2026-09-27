//! A bare text window for the OS e2e suite to dictate into.
//!
//! TextEdit made a flaky target: its autocorrect bubble and open panels
//! grab focus or block AppleEvents, and it holds the user's own documents.
//! This window has none of that, and reports its text by writing it to the
//! file given as argv[1] on every change, so tests read it back without
//! scripting the app at all.

#[cfg(target_os = "macos")]
fn main() {
    use objc2::rc::autoreleasepool;
    use objc2::{sel, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSEventMask, NSMenu, NSMenuItem,
        NSTextView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{ns_string, NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSSize};

    let out = std::env::args().nth(1).expect("usage: typing-target <out-file>");
    let _ = std::fs::write(&out, "");
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    // An Edit menu, so ⌘V reaches the text view's paste: like in any app.
    let menubar = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let edit_item = NSMenuItem::new(mtm);
    menubar.addItem(&app_item);
    menubar.addItem(&edit_item);
    let edit = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Edit"));
    let paste = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Paste"),
            Some(sel!(paste:)),
            ns_string!("v"),
        )
    };
    edit.addItem(&paste);
    edit_item.setSubmenu(Some(&edit));
    app.setMainMenu(Some(&menubar));

    let frame = NSRect::new(NSPoint::new(240.0, 240.0), NSSize::new(520.0, 260.0));
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(ns_string!("hearme e2e target"));
    let tv = NSTextView::initWithFrame(NSTextView::alloc(mtm), frame);
    tv.setRichText(false);
    tv.setAutomaticSpellingCorrectionEnabled(false);
    tv.setContinuousSpellCheckingEnabled(false);
    tv.setAutomaticTextReplacementEnabled(false);
    tv.setAutomaticQuoteSubstitutionEnabled(false);
    tv.setAutomaticDashSubstitutionEnabled(false);
    tv.setAutomaticTextCompletionEnabled(false);
    window.setContentView(Some(&tv));
    window.makeKeyAndOrderFront(None);
    window.makeFirstResponder(Some(&tv));
    app.finishLaunching();
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    // A hand-rolled run loop: pump events, mirror the text to the file.
    let mut last = String::new();
    loop {
        autoreleasepool(|_| {
            let until = NSDate::dateWithTimeIntervalSinceNow(0.05);
            let ev = unsafe {
                app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSEventMask::Any,
                    Some(&until),
                    NSDefaultRunLoopMode,
                    true,
                )
            };
            if let Some(ev) = ev {
                app.sendEvent(&ev);
            }
            let s = tv.string().to_string();
            if s != last {
                let _ = std::fs::write(&out, &s);
                last = s;
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
