/// NSView subclass hosting a CAMetalLayer, frosted glass background, and display link.
import AppKit
import Metal
import QuartzCore
import WolpaBridge

public final class RenderView: NSView {

    public var metalLayer: CAMetalLayer!
    public var effectView: NSVisualEffectView!
    public var ctx: OpaquePointer?
    public var displayLink: CVDisplayLink?
    private var ctxRef: OpaquePointer?
    private var cols: UInt64 = 80
    private var rows: UInt64 = 24

    public override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        setupLayers()
    }

    public required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
        setupLayers()
    }

    private func setupLayers() {
        // Frosted glass background
        effectView = NSVisualEffectView(frame: bounds)
        effectView.autoresizingMask = [.width, .height]
        effectView.blendingMode = .behindWindow
        effectView.material = .dark
        effectView.state = .active
        addSubview(effectView)

        // Metal layer on top
        metalLayer = CAMetalLayer()
        metalLayer.device = MTLCreateSystemDefaultDevice()
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = false
        metalLayer.frame = bounds
        metalLayer.isOpaque = false
        metalLayer.backgroundColor = CGColor(red: 0, green: 0, blue: 0, alpha: 0)
        wantsLayer = true
        layer?.addSublayer(metalLayer)
    }

    override public var acceptsFirstResponder: Bool { true }

    override public func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        window?.makeFirstResponder(self)

        let scale = Double(metalLayer.contentsScale)
        ctx = Bridge.initialize(layer: metalLayer, cols: cols, rows: rows, scale: scale)
        ctxRef = ctx

        updateDrawableSize()
        startDisplayLink()
    }

    override public func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        metalLayer.frame = bounds
        effectView.frame = bounds
        updateDrawableSize()
    }

    private func updateDrawableSize() {
        let scale = Double(metalLayer.contentsScale)
        var cw: Double = 0
        var ch: Double = 0
        if let c = ctx {
            wolpa_get_cell_size(c, &cw, &ch)
        }
        if cw <= 0 { cw = 8 * scale }
        if ch <= 0 { ch = 16 * scale }

        let pw = CGFloat(cols) * CGFloat(cw) + 4
        let ph = CGFloat(rows) * CGFloat(ch) + 4
        if bounds.size.width > 0 && bounds.size.height > 0 {
            metalLayer.drawableSize = CGSize(width: pw, height: ph)
        }
    }

    // MARK: - Display Link

    private func startDisplayLink() {
        let callback: CVDisplayLinkOutputCallback = { _, _, _, _, _, ctxPtr in
            let ctx = ctxPtr!.assumingMemoryBound(to: OpaquePointer.self).pointee
            wolpa_render(ctx)
            return kCVReturnSuccess
        }

        CVDisplayLinkCreateWithActiveCGDisplays(&displayLink)
        CVDisplayLinkSetOutputCallback(displayLink!, callback, &ctxRef)
        CVDisplayLinkStart(displayLink!)
    }

    // MARK: - Keyboard Input

    override public func keyDown(with event: NSEvent) {
        guard let ctx = ctx else {
            super.keyDown(with: event)
            return
        }
        let input = translateKeyEvent(event)
        if !input.isEmpty {
            input.withCString { ptr in
                wolpa_input(ctx, ptr)
            }
        }
    }

    private func translateKeyEvent(_ event: NSEvent) -> String {
        let chars = event.characters ?? ""
        let modifiers = event.modifierFlags

        if modifiers.contains(.command) { return "" }

        switch event.keyCode {
        case 36: return "\r"
        case 48: return "\t"
        case 51: return "\u{7f}"
        case 53: return "\u{1b}"
        case 123: return "\u{1b}[D"
        case 124: return "\u{1b}[C"
        case 125: return "\u{1b}[B"
        case 126: return "\u{1b}[A"
        default: break
        }

        var result = ""
        if modifiers.contains(.control) { result += "<C-" }
        if modifiers.contains(.option) { result += "<M-" }

        if !chars.isEmpty {
            let ch = String(chars.first!)
            if ch == " " { return " " }
            if result.isEmpty { return ch }
            result += ch + ">"
            return result
        }
        return chars
    }

    public func stop() {
        CVDisplayLinkStop(displayLink!)
        if let ctx { wolpa_destroy(ctx) }
    }
}
