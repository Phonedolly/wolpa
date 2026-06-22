/// WolpaApp — macOS native Neovim GUI entry point.
import AppKit
import WolpaView

final class AppDelegate: NSObject, NSApplicationDelegate {

    var window: NSWindow!
    var view: RenderView!

    func applicationDidFinishLaunching(_ notification: Notification) {
        let width: CGFloat = 800
        let height: CGFloat = 500

        let rect = NSRect(x: 200, y: 200, width: width, height: height)
        window = NSWindow(
            contentRect: rect,
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Wolpa"
        window.makeKeyAndOrderFront(nil)

        view = RenderView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        window.contentView = view
    }

    func applicationWillTerminate(_ notification: Notification) {
        view?.stop()
    }
}

NSApplication.shared.setActivationPolicy(.regular)
let delegate = AppDelegate()
NSApplication.shared.delegate = delegate
NSApplication.shared.run()
