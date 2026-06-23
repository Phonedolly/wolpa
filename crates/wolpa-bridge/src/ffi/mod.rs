#![allow(clippy::missing_safety_doc)]
//! ## FFI function declarations
//!
//! C-callable entry points consumed by the Swift AppKit layer.

use std::ffi::c_void;

use tokio::runtime::Runtime;
use wolpa_core::grid::GridState;
use wolpa_core::highlight::HighlightResolver;
use wolpa_core::rpc::RpcClient;
use wolpa_render::font::Font;
use wolpa_render::metal::MetalRenderer;

pub struct WolpaContext {
    pub renderer: MetalRenderer,
    pub font: Font,
    pub runtime: Runtime,
    pub client: RpcClient,
    pub grid: GridState,
    pub highlight: HighlightResolver,
    pub frame_count: u64,
    pub cols: u64,
    pub rows: u64,
    pub scale: f64,
    input_queue: Vec<String>,
}

#[no_mangle]
pub unsafe extern "C" fn wolpa_init(
    layer: *mut c_void,
    cols: u64,
    rows: u64,
    scale: f64,
) -> *mut WolpaContext {
    let renderer = MetalRenderer::from_raw_layer(layer, cols, rows);
    let font = Font::new(14.0 * scale, scale);
    let runtime = Runtime::new().expect("tokio runtime");

    let (client, grid, highlight) = runtime.block_on(async {
        let mut client = RpcClient::spawn().await.expect("nvim embed");
        client.ui_attach(cols, rows).await.expect("ui_attach");
        let events = client.drain_events().await.unwrap();
        let mut grid = GridState::new(cols, rows);
        let mut highlight = HighlightResolver::new();
        apply_events(&mut grid, &mut highlight, &events);

        // Smoke test: insert visible text to verify rendering works
        client.input("iHello\x1b").await.ok();
        client.redraw_flush().await.ok();
        let events = client.drain_events().await.unwrap();
        apply_events(&mut grid, &mut highlight, &events);

        (client, grid, highlight)
    });

    Box::into_raw(Box::new(WolpaContext {
        renderer,
        font,
        runtime,
        client,
        grid,
        highlight,
        frame_count: 0,
        cols,
        rows,
        scale,
        input_queue: Vec::new(),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn wolpa_render(ctx: *mut WolpaContext) {
    if ctx.is_null() {
        return;
    }
    let ctx = &mut *ctx;
    ctx.frame_count += 1;

    ctx.runtime.block_on(async {
        if let Ok(events) = ctx.client.drain_events().await {
            apply_events(&mut ctx.grid, &mut ctx.highlight, &events);
        }
        for keys in ctx.input_queue.drain(..) {
            ctx.client.input(&keys).await.ok();
        }
    });

    ctx.renderer
        .render_grid(&ctx.grid, &ctx.highlight, &ctx.font.metrics, &ctx.font);
}

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

#[no_mangle]
pub unsafe extern "C" fn wolpa_input(
    ctx: *mut WolpaContext,
    keys_ptr: *const std::ffi::c_char,
) -> bool {
    if ctx.is_null() || keys_ptr.is_null() {
        return false;
    }
    unsafe {
        (*ctx).input_queue.push(
            std::ffi::CStr::from_ptr(keys_ptr)
                .to_string_lossy()
                .into_owned(),
        );
    }
    true
}

#[no_mangle]
pub unsafe extern "C" fn wolpa_get_cell_size(
    ctx: *const WolpaContext,
    width: *mut f64,
    height: *mut f64,
) {
    if ctx.is_null() || width.is_null() || height.is_null() {
        return;
    }
    *width = (*ctx).font.metrics.width;
    *height = (*ctx).font.metrics.height;
}

#[no_mangle]
pub unsafe extern "C" fn wolpa_mouse(
    ctx: *mut WolpaContext,
    button: *const std::ffi::c_char,
    action: *const std::ffi::c_char,
    row: u64,
    col: u64,
) -> bool {
    if ctx.is_null() || button.is_null() || action.is_null() {
        return false;
    }
    let btn = unsafe { std::ffi::CStr::from_ptr(button) }.to_string_lossy();
    let act = unsafe { std::ffi::CStr::from_ptr(action) }.to_string_lossy();
    (*ctx).runtime.block_on(async {
        (*ctx)
            .client
            .call(
                "nvim_input_mouse",
                vec![
                    rmpv::Value::String(btn.as_ref().into()),
                    rmpv::Value::String(act.as_ref().into()),
                    rmpv::Value::String("".into()),
                    rmpv::Value::from(2u64),
                    rmpv::Value::from(row),
                    rmpv::Value::from(col),
                ],
            )
            .await
            .is_ok()
    })
}

#[no_mangle]
pub unsafe extern "C" fn wolpa_set_font_size(ctx: *mut WolpaContext, pt_size: f64) -> bool {
    if ctx.is_null() {
        return false;
    }
    (*ctx).font = Font::new(pt_size * (*ctx).scale, (*ctx).scale);
    (*ctx).runtime.block_on(async {
        (*ctx)
            .client
            .call(
                "nvim_ui_try_resize",
                vec![
                    rmpv::Value::from((*ctx).cols),
                    rmpv::Value::from((*ctx).rows),
                ],
            )
            .await
            .is_ok()
    })
}

#[no_mangle]
pub unsafe extern "C" fn wolpa_resize(ctx: *mut WolpaContext, cols: u64, rows: u64) -> bool {
    if ctx.is_null() {
        return false;
    }
    (*ctx).cols = cols;
    (*ctx).rows = rows;
    (*ctx).runtime.block_on(async {
        (*ctx)
            .client
            .call(
                "nvim_ui_try_resize",
                vec![rmpv::Value::from(cols), rmpv::Value::from(rows)],
            )
            .await
            .is_ok()
    })
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
