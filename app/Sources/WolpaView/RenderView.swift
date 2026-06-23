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
    private var fontPtSize: Double = 14.0

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
        effectView = NSVisualEffectView(frame: bounds)
        effectView.autoresizingMask = [.width, .height]
        effectView.blendingMode = .behindWindow
        effectView.material = .dark
        effectView.state = .active
        addSubview(effectView)

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
        window?.acceptsMouseMovedEvents = true

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

        // Recompute grid size based on new view size
        var cw: Double = 0; var ch: Double = 0
        if let c = ctx { wolpa_get_cell_size(c, &cw, &ch) }
        if cw > 0 && ch > 0 {
            let newCols = max(20, UInt64((bounds.width - 4) / CGFloat(cw)))
            let newRows = max(5, UInt64((bounds.height - 4) / CGFloat(ch)))
            if newCols != cols || newRows != rows {
                cols = newCols; rows = newRows
                if let c = ctx { wolpa_resize(c, cols, rows) }
            }
        }
        updateDrawableSize()
    }

    private func updateDrawableSize() {
        let dscale = Double(metalLayer.contentsScale)
        var cw: Double = 0; var ch: Double = 0
        if let c = ctx { wolpa_get_cell_size(c, &cw, &ch) }
        if cw <= 0 { cw = 8 * dscale }
        if ch <= 0 { ch = 16 * dscale }

        let pw = CGFloat(cols) * CGFloat(cw) + 4
        let ph = CGFloat(rows) * CGFloat(ch) + 4
        if bounds.size.width > 0 && bounds.size.height > 0 {
            metalLayer.drawableSize = CGSize(width: pw, height: ph)
        }
    }

    // MARK: - Mouse Events

    private func viewToGrid(_ point: NSPoint) -> (UInt64, UInt64) {
        var cw: Double = 0; var ch: Double = 0
        if let c = ctx { wolpa_get_cell_size(c, &cw, &ch) }
        if cw <= 0 { cw = 10 }; if ch <= 0 { ch = 20 }
        let col = UInt64(max(0, (point.x - 2) / CGFloat(cw)))
        let row = UInt64(max(0, (point.y - 2) / CGFloat(ch)))
        return (min(row, rows - 1), min(col, cols - 1))
    }

    override public func mouseDown(with event: NSEvent) {
        guard let c = ctx else { return }
        let (row, col) = viewToGrid(convert(event.locationInWindow, from: nil))
        "left".withCString { b in "press".withCString { a in wolpa_mouse(c, b, a, row, col) } }
    }

    override public func mouseUp(with event: NSEvent) {
        guard let c = ctx else { return }
        let (row, col) = viewToGrid(convert(event.locationInWindow, from: nil))
        "left".withCString { b in "release".withCString { a in wolpa_mouse(c, b, a, row, col) } }
    }

    override public func mouseDragged(with event: NSEvent) {
        guard let c = ctx else { return }
        let (row, col) = viewToGrid(convert(event.locationInWindow, from: nil))
        "left".withCString { b in "drag".withCString { a in wolpa_mouse(c, b, a, row, col) } }
    }

    override public func rightMouseDown(with event: NSEvent) {
        guard let c = ctx else { return }
        let (row, col) = viewToGrid(convert(event.locationInWindow, from: nil))
        "right".withCString { b in "press".withCString { a in wolpa_mouse(c, b, a, row, col) } }
    }

    override public func scrollWheel(with event: NSEvent) {
        guard let c = ctx else { return }
        let (row, col) = viewToGrid(convert(event.locationInWindow, from: nil))

        // Scroll wheel: send multiple events for larger deltas
        let dy = event.deltaY
        let dx = event.deltaX
        let dirY: (String, UInt64) = dy > 0 ? ("up", UInt64(abs(dy).rounded(.up))) : ("down", UInt64(abs(dy).rounded(.up)))
        let dirX: (String, UInt64) = dx > 0 ? ("left", UInt64(abs(dx).rounded(.up))) : ("right", UInt64(abs(dx).rounded(.up)))

        for _ in 0..<dirX.1 {
            dirX.0.withCString { b in "press".withCString { a in wolpa_mouse(c, b, a, row, col) } }
        }
        for _ in 0..<dirY.1 {
            dirY.0.withCString { b in "press".withCString { a in wolpa_mouse(c, b, a, row, col) } }
        }
    }

    // MARK: - Keyboard Input

    override public func keyDown(with event: NSEvent) {
        guard let c = ctx else { super.keyDown(with: event); return }

        // Font resize: Cmd+= or Cmd+Shift+=
        if event.modifierFlags.contains(.command) {
            switch event.keyCode {
            case 24: // Cmd+= (actually Cmd+Shift+= on US keyboard, but keyCode 24 is =)
                fontPtSize = min(32, fontPtSize + 2)
                wolpa_set_font_size(c, fontPtSize)
                updateDrawableSize()
                return
            case 27: // Cmd+- (minus)
                fontPtSize = max(8, fontPtSize - 2)
                wolpa_set_font_size(c, fontPtSize)
                updateDrawableSize()
                return
            default: break
            }
        }

        let input = translateKeyEvent(event)
        if !input.isEmpty {
            input.withCString { ptr in wolpa_input(c, ptr) }
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

    public func stop() {
        CVDisplayLinkStop(displayLink!)
        if let ctx { wolpa_destroy(ctx) }
    }
}
