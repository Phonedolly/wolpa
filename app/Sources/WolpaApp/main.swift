import AppKit

@main
struct WolpaApp {
    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.regular)
        app.run()
    }
}
