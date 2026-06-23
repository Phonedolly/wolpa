/// WolpaApp — macOS native Neovim GUI entry point.
import AppKit
import WolpaView

final class AppDelegate: NSObject, NSApplicationDelegate {

    var window: NSWindow!
    var view: RenderView!

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupMenu()

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
        window.makeFirstResponder(view)
    }

    func applicationWillTerminate(_ notification: Notification) {
        view?.stop()
    }

    private func setupMenu() {
        let mainMenu = NSMenu(title: "MainMenu")

        // App menu
        let appMenuItem = NSMenuItem()
        mainMenu.addItem(appMenuItem)
        let appMenu = NSMenu(title: "Wolpa")
        appMenu.addItem(NSMenuItem(title: "Quit Wolpa", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        appMenuItem.submenu = appMenu

        NSApplication.shared.mainMenu = mainMenu
    }
}

NSApplication.shared.setActivationPolicy(.regular)
let delegate = AppDelegate()
NSApplication.shared.delegate = delegate
NSApplication.shared.run()
