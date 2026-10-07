// iboard-catalog: show the design catalog in a live WebKit window.
//
//   iboard-catalog <catalog.html> [title]
//
// It runs as its own process, so the board never loads WebKit itself: the
// catalog is fully interactive (hover, focus, search, the theme switch) while
// a crash can only take this window down. Exits when its window closes.

import AppKit
import WebKit

let arguments = CommandLine.arguments
guard arguments.count >= 2 else {
    FileHandle.standardError.write(
        "usage: iboard-catalog <catalog.html> [title]\n".data(using: .utf8)!)
    exit(2)
}
let catalogURL = URL(fileURLWithPath: arguments[1])
let title = arguments.count > 2 ? arguments[2] : "Design catalog"

let app = NSApplication.shared
// A regular app: it gets a Dock icon and can take focus, unlike the snapshot
// helper which must stay invisible.
app.setActivationPolicy(.regular)

final class Delegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var window: NSWindow?

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

let delegate = Delegate()
app.delegate = delegate

// A minimal menu so `⌘Q` quits and the window behaves like a normal app.
let mainMenu = NSMenu()
let appMenuItem = NSMenuItem()
mainMenu.addItem(appMenuItem)
let appMenu = NSMenu()
appMenu.addItem(
    withTitle: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
appMenuItem.submenu = appMenu
app.mainMenu = mainMenu

let configuration = WKWebViewConfiguration()
let webView = WKWebView(
    frame: NSRect(x: 0, y: 0, width: 1120, height: 780), configuration: configuration)

let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: 1120, height: 780),
    styleMask: [.titled, .closable, .miniaturizable, .resizable],
    backing: .buffered, defer: false)
window.title = title
window.contentView = webView
window.setFrameAutosaveName("IboardCatalog")
window.center()
window.delegate = delegate
delegate.window = window
window.makeKeyAndOrderFront(nil)

webView.loadFileURL(catalogURL, allowingReadAccessTo: catalogURL.deletingLastPathComponent())
app.activate(ignoringOtherApps: true)
app.run()
