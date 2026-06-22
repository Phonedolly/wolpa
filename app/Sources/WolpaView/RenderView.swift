/// NSView subclass hosting a CAMetalLayer and a display link render loop.
import AppKit
import Metal
import QuartzCore
import WolpaBridge

public final class RenderView: NSView {

    public var metalLayer: CAMetalLayer!
    public var ctx: OpaquePointer?
    public var displayLink: CVDisplayLink?

    public override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        setupMetal()
    }

    public required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
        setupMetal()
    }

    private func setupMetal() {
        metalLayer = CAMetalLayer()
        metalLayer.device = MTLCreateSystemDefaultDevice()
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = false
        metalLayer.frame = bounds
        layer = metalLayer
    }

    override public func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        metalLayer.frame = bounds

        let cols: UInt64 = 80
        let rows: UInt64 = 24
        ctx = Bridge.initialize(layer: metalLayer, cols: cols, rows: rows)

        startDisplayLink()
    }

    override public func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        metalLayer.frame = bounds
        metalLayer.drawableSize = convertToBacking(bounds).size
    }

    private func startDisplayLink() {
        let callback: CVDisplayLinkOutputCallback = { _, _, _, _, _, ctxPtr in
            let ctx = ctxPtr!.assumingMemoryBound(to: OpaquePointer.self).pointee
            wolpa_render(ctx)
            return kCVReturnSuccess
        }

        var ctxRef = ctx
        CVDisplayLinkCreateWithActiveCGDisplays(&displayLink)
        CVDisplayLinkSetOutputCallback(
            displayLink!, callback, &ctxRef
        )
        CVDisplayLinkStart(displayLink!)
    }

    public func stop() {
        CVDisplayLinkStop(displayLink!)
        if let ctx {
            wolpa_destroy(ctx)
        }
    }
}
