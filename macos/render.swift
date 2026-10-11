// iboard-render: snapshot an HTML file to a PNG using the system's WebKit.
//
//   iboard-render <input.html> <output.png> [width] [height]
//
// It runs as a short-lived helper process, so iboard never loads WebKit into
// its own process: the app only ever sees a PNG. Exits 0 on success; writes a
// message to stderr and exits non-zero otherwise, and gives up after a timeout.

import AppKit
import WebKit

let arguments = CommandLine.arguments
guard arguments.count >= 3 else {
    FileHandle.standardError.write(
        "usage: iboard-render <input.html> <output.png> [width] [height]\n".data(using: .utf8)!)
    exit(2)
}

let inputURL = URL(fileURLWithPath: arguments[1])
let outputURL = URL(fileURLWithPath: arguments[2])
let width = arguments.count > 3 ? (Double(arguments[3]) ?? 1200.0) : 1200.0
let height = arguments.count > 4 ? (Double(arguments[4]) ?? 900.0) : 900.0

func fail(_ message: String) -> Never {
    FileHandle.standardError.write("iboard-render: \(message)\n".data(using: .utf8)!)
    exit(1)
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)

let configuration = WKWebViewConfiguration()
// Previews are static, and an agent may have written them: no scripts run.
configuration.defaultWebpagePreferences.allowsContentJavaScript = false
let webView = WKWebView(
    frame: NSRect(x: 0, y: 0, width: width, height: height), configuration: configuration)

// A borderless, off-screen window gives the web view something to draw into.
// It is never ordered on screen.
let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: width, height: height),
    styleMask: [.borderless], backing: .buffered, defer: false)
window.contentView = webView

final class Delegate: NSObject, WKNavigationDelegate {
    let outputURL: URL
    let webView: WKWebView
    var finished = false

    init(outputURL: URL, webView: WKWebView) {
        self.outputURL = outputURL
        self.webView = webView
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        // Let fonts and layout settle before the snapshot.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { self.snapshot() }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        fail("load failed: \(error.localizedDescription)")
    }

    func webView(
        _ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        fail("load failed: \(error.localizedDescription)")
    }

    func snapshot() {
        guard !finished else { return }
        finished = true
        let config = WKSnapshotConfiguration()
        config.rect = CGRect(x: 0, y: 0, width: webView.frame.width, height: webView.frame.height)
        webView.takeSnapshot(with: config) { image, error in
            if let error = error {
                fail("snapshot failed: \(error.localizedDescription)")
            }
            guard let image = image,
                let tiff = image.tiffRepresentation,
                let representation = NSBitmapImageRep(data: tiff),
                let png = representation.representation(using: .png, properties: [:])
            else {
                fail("could not encode the snapshot")
            }
            do {
                try png.write(to: self.outputURL)
            } catch {
                fail("could not write \(self.outputURL.path): \(error.localizedDescription)")
            }
            exit(0)
        }
    }
}

let delegate = Delegate(outputURL: outputURL, webView: webView)
webView.navigationDelegate = delegate
webView.loadFileURL(inputURL, allowingReadAccessTo: inputURL.deletingLastPathComponent())

DispatchQueue.main.asyncAfter(deadline: .now() + 20.0) { fail("timed out") }

app.run()
