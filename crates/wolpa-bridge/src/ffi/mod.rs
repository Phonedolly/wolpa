//! ## FFI function declarations
//!
//! C-callable entry points consumed by the Swift AppKit layer.
//! Delegates to `wolpa-render` and `wolpa-core`. No business logic.

use std::ffi::c_void;

use tokio::runtime::Runtime;
use wolpa_core::grid::GridState;
use wolpa_core::highlight::HighlightResolver;
use wolpa_core::rpc::RpcClient;
use wolpa_render::font::Font;
use wolpa_render::metal::MetalRenderer;

/// Opaque context holding renderer, nvim client, and grid state.
pub struct WolpaContext {
    pub renderer: MetalRenderer,
    pub font: Font,
    pub runtime: Runtime,
    pub client: RpcClient,
    pub grid: GridState,
    pub highlight: HighlightResolver,
    pub frame_count: u64,
}

/// Create a new Wolpa context: spawns nvim, attaches UI, renders.
///
/// # Safety
///
/// `layer` must be a valid, retained `CAMetalLayer *`.
#[no_mangle]
pub unsafe extern "C" fn wolpa_init(layer: *mut c_void, cols: u64, rows: u64) -> *mut WolpaContext {
    let renderer = MetalRenderer::from_raw_layer(layer, cols, rows);
    let font = Font::new(14.0);

    // Create a tokio runtime for async nvim I/O
    let runtime = Runtime::new().expect("tokio runtime");

    // Spawn nvim and attach
    let (client, grid, highlight) = runtime.block_on(async {
        let mut client = RpcClient::spawn().await.expect("nvim --embed spawn");
        client.ui_attach(cols, rows).await.expect("ui_attach");

        // Drain initial events
        let events = client.drain_events().await.unwrap();
        let mut grid = GridState::new(cols, rows);
        let mut highlight = HighlightResolver::new();
        apply_events(&mut grid, &mut highlight, &events);

        (client, grid, highlight)
    });

    let ctx = Box::new(WolpaContext {
        renderer,
        font,
        runtime,
        client,
        grid,
        highlight,
        frame_count: 0,
    });
    Box::into_raw(ctx)
}

/// Render one frame: drain nvim events, build quads, submit draw.
///
/// # Safety
///
/// `ctx` must be a valid pointer returned by `wolpa_init`.
#[no_mangle]
pub unsafe extern "C" fn wolpa_render(ctx: *mut WolpaContext) {
    if ctx.is_null() {
        return;
    }
    let ctx = &mut *ctx;
    ctx.frame_count += 1;

    // Drain any pending nvim events
    ctx.runtime.block_on(async {
        if let Ok(events) = ctx.client.drain_events().await {
            apply_events(&mut ctx.grid, &mut ctx.highlight, &events);
        }
    });

    // Build cell data from grid and render
    ctx.renderer
        .render_grid(&ctx.grid, &ctx.highlight, &ctx.font.metrics, &ctx.font);
}

/// Destroy the context and free all resources.
///
/// # Safety
///
/// `ctx` must be a valid pointer returned by `wolpa_init`.
#[no_mangle]
pub unsafe extern "C" fn wolpa_destroy(ctx: *mut WolpaContext) {
    if ctx.is_null() {
        return;
    }
    let mut ctx = Box::from_raw(ctx);
    ctx.runtime.block_on(async {
        ctx.client.shutdown().await.ok();
    });
}

fn apply_events(
    grid: &mut GridState,
    highlight: &mut HighlightResolver,
    events: &[wolpa_core::event::UiEvent],
) {
    for event in events {
        if let wolpa_core::event::UiEvent::Redraw(ref redraw_events) = event {
            grid.apply_batch(redraw_events);
            for e in redraw_events {
                highlight.apply(e);
            }
        }
    }
}

/// Send input to nvim. Returns true if nvim processed it.
///
/// # Safety
///
/// `ctx` must be a valid pointer from `wolpa_init`.
/// `keys_ptr` must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn wolpa_input(
    ctx: *mut WolpaContext,
    keys_ptr: *const std::ffi::c_char,
) -> bool {
    if ctx.is_null() || keys_ptr.is_null() {
        return false;
    }
    let ctx = &mut *ctx;
    let keys = unsafe { std::ffi::CStr::from_ptr(keys_ptr) }.to_string_lossy();
    ctx.runtime
        .block_on(async { ctx.client.input(&keys).await.is_ok() })
}
