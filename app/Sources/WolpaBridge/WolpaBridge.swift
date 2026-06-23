/// Wraps Rust FFI calls from wolpa-bridge.
/// Uses @_silgen_name to directly link against the Rust symbols.
import AppKit
import Metal

// Direct FFI imports from libwolpa_bridge.a
@_silgen_name("wolpa_init")
public func wolpa_init(_ layer: UnsafeMutableRawPointer, _ cols: UInt64, _ rows: UInt64, _ scale: Double) -> OpaquePointer?

@_silgen_name("wolpa_render")
public func wolpa_render(_ ctx: OpaquePointer)

@_silgen_name("wolpa_destroy")
public func wolpa_destroy(_ ctx: OpaquePointer)

@_silgen_name("wolpa_input")
public func wolpa_input(_ ctx: OpaquePointer, _ keys: UnsafePointer<CChar>) -> Bool

@_silgen_name("wolpa_get_cell_size")
public func wolpa_get_cell_size(_ ctx: OpaquePointer, _ width: UnsafeMutablePointer<Double>, _ height: UnsafeMutablePointer<Double>)

@_silgen_name("wolpa_resize")
public func wolpa_resize(_ ctx: OpaquePointer, _ cols: UInt64, _ rows: UInt64) -> Bool

public enum Bridge {

    /// Initialize the renderer with a CAMetalLayer.
    /// Returns an opaque pointer to the Rust-side WolpaContext.
    public static func initialize(layer: CAMetalLayer, cols: UInt64, rows: UInt64, scale: Double) -> OpaquePointer? {
        let ptr = Unmanaged.passUnretained(layer).toOpaque()
        return wolpa_init(ptr, cols, rows, scale)
    }

    static func render(_ ctx: OpaquePointer) {
        wolpa_render(ctx)
    }

    static func destroy(_ ctx: OpaquePointer) {
        wolpa_destroy(ctx)
    }
}
