//! ## FFI function declarations
//!
//! C-callable entry points consumed by the Swift AppKit layer.
//! These functions delegate to `wolpa-render` and `wolpa-core`.
//! No business logic lives here.

use std::ffi::c_void;

use wolpa_render::metal::MetalRenderer;

/// Opaque context holding all renderer and nvim state.
pub struct WolpaContext {
    pub renderer: MetalRenderer,
}

/// Create a new Wolpa context from a CAMetalLayer pointer.
///
/// Returns a heap-allocated context. The caller owns the pointer
/// and must call `wolpa_destroy` to free it.
///
/// # Safety
///
/// `layer` must be a valid, retained `CAMetalLayer *`.
#[no_mangle]
pub unsafe extern "C" fn wolpa_init(layer: *mut c_void, cols: u64, rows: u64) -> *mut WolpaContext {
    let renderer = MetalRenderer::from_raw_layer(layer, cols, rows);
    let ctx = Box::new(WolpaContext { renderer });
    Box::into_raw(ctx)
}

/// Render one frame.
///
/// # Safety
///
/// `ctx` must be a valid pointer returned by `wolpa_init` and must not
/// have been freed by `wolpa_destroy`.
#[no_mangle]
pub unsafe extern "C" fn wolpa_render(ctx: *mut WolpaContext) {
    if ctx.is_null() {
        return;
    }
    let ctx = &*ctx;
    ctx.renderer.render_frame();
}

/// Destroy the context and free all resources.
///
/// # Safety
///
/// `ctx` must be a valid pointer returned by `wolpa_init`.
/// After this call, the pointer is invalid.
#[no_mangle]
pub unsafe extern "C" fn wolpa_destroy(ctx: *mut WolpaContext) {
    if ctx.is_null() {
        return;
    }
    let _ = Box::from_raw(ctx);
}
