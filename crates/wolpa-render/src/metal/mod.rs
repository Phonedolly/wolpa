//! ## Metal renderer
//!
//! Converts grid state into Metal draw commands and submits them to the GPU.

use foreign_types::ForeignTypeRef;
use objc::rc::autoreleasepool;
use objc::{msg_send, sel, sel_impl};

use crate::font::CellMetrics;
use crate::layout::Layout;
use wolpa_core::grid::GridState;
use wolpa_core::highlight::HighlightResolver;

/// Metal render state: device, shaders, pipeline, command queue, layer object.
pub struct MetalRenderer {
    pub device: metal::Device,
    pub command_queue: metal::CommandQueue,
    pub pipeline: metal::RenderPipelineState,
    pub bg_pipeline: metal::RenderPipelineState,
    pub vertex_buffer: metal::Buffer,
    pub color_buffer: metal::Buffer,
    layer: *mut objc::runtime::Object,
    pub layout: Layout,
    pub cols: u64,
    pub rows: u64,
}

/// Vertex data for a single quad (cell): position (x, y) + UV (u, v).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct QuadVertex {
    pub position: [f32; 2],
    pub texcoord: [f32; 2],
}

/// Per-instance uniform: foreground and background colors.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CellColor {
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

/// Metal Shading Language source.
const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 uv;
};

struct CellColor {
    float4 fg;
    float4 bg;
};

// ── Background pass ─────────────────────────────────────────────

vertex VertexOut bg_vertex(
    uint vertexID [[vertex_id]],
    constant float2* positions [[buffer(0)]],
    constant float2* uvs [[buffer(1)]]
) {
    VertexOut out;
    out.position = float4(positions[vertexID], 0.0, 1.0);
    out.uv = uvs[vertexID];
    return out;
}

fragment float4 bg_fragment(
    VertexOut in [[stage_in]],
    uint instanceID [[instance_id]],
    constant CellColor* colors [[buffer(0)]]
) {
    return colors[instanceID].bg;
}
"#;

impl MetalRenderer {
    /// Create a Metal rendering context from a raw CAMetalLayer pointer.
    ///
    /// # Safety
    ///
    /// `layer_ptr` must be a valid, retained `CAMetalLayer *` that outlives
    /// the renderer. The layer must have a valid MTLDevice set.
    pub unsafe fn from_raw_layer(layer_ptr: *mut std::ffi::c_void, cols: u64, rows: u64) -> Self {
        let layer = layer_ptr as *mut objc::runtime::Object;
        let _layer_dev: *mut objc::runtime::Object = msg_send![layer, device];
        let device = metal::Device::system_default().expect("no Metal GPU");
        let command_queue = device.new_command_queue();

        let options = metal::CompileOptions::new();
        let library = device
            .new_library_with_source(SHADER_SOURCE, &options)
            .expect("failed to compile Metal shaders");

        // Background pass pipeline
        let bg_vertex = library.get_function("bg_vertex", None).expect("bg_vertex");
        let bg_fragment = library
            .get_function("bg_fragment", None)
            .expect("bg_fragment");

        let bg_desc = metal::RenderPipelineDescriptor::new();
        bg_desc.set_vertex_function(Some(&bg_vertex));
        bg_desc.set_fragment_function(Some(&bg_fragment));
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        let bg_pipeline = device
            .new_render_pipeline_state(&bg_desc)
            .expect("bg pipeline state");

        // Keep a simple pipeline for reference
        let pipeline = bg_pipeline.clone();

        let num_cells = (cols * rows) as usize;
        let vbuf_size = (num_cells * 6 * std::mem::size_of::<QuadVertex>()) as u64;
        let cbuf_size = (num_cells * std::mem::size_of::<CellColor>()) as u64;

        let vertex_buffer =
            device.new_buffer(vbuf_size, metal::MTLResourceOptions::StorageModeShared);
        let color_buffer =
            device.new_buffer(cbuf_size, metal::MTLResourceOptions::StorageModeShared);

        let dummy_metrics = CellMetrics {
            width: 8.0,
            height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
        };
        let layout = Layout::new(cols, rows, &dummy_metrics);

        let _: () = msg_send![layer, setPixelFormat: metal::MTLPixelFormat::BGRA8Unorm as u64];

        MetalRenderer {
            device,
            command_queue,
            pipeline,
            bg_pipeline,
            vertex_buffer,
            color_buffer,
            layer,
            layout,
            cols,
            rows,
        }
    }

    /// Render grid cells: build quads from grid state and draw background colors.
    pub fn render_grid(
        &mut self,
        grid: &GridState,
        highlight: &HighlightResolver,
        metrics: &CellMetrics,
    ) {
        // Update layout with real font metrics
        self.layout = Layout::new(self.cols, self.rows, metrics);

        // Collect visible cells across all grids
        let mut all_cells: Vec<(f64, f64, f64, f64, [f32; 4])> = Vec::new();

        // For ext_multigrid, render each window grid at its win_pos
        for (&grid_id, g) in &grid.grids {
            if grid_id == 1 {
                // Grid 1 is the root — skip it, render child grids only
                continue;
            }
            let win_start_row = 0u64;
            let win_start_col = 0u64;

            for row in 0..g.height {
                for col in 0..g.width {
                    let cell = g.cell(row, col);
                    let attr = highlight.resolve(cell.hl_id);
                    let bg = attr.background.unwrap_or([0.0, 0.0, 0.0, 1.0]);

                    let (px, py) = self
                        .layout
                        .cell_to_pixel(win_start_col + col, win_start_row + row);
                    let pw = self.layout.cell_width;
                    let ph = self.layout.cell_height;

                    all_cells.push((px, py, pw, ph, bg));
                }
            }
        }

        // If no child grids, render grid 1 as fallback
        if all_cells.is_empty() {
            if let Some(g) = grid.grid(1) {
                for row in 0..g.height {
                    for col in 0..g.width {
                        let cell = g.cell(row, col);
                        let attr = highlight.resolve(cell.hl_id);
                        let bg = attr.background.unwrap_or([0.0, 0.0, 0.0, 1.0]);

                        let (px, py) = self.layout.cell_to_pixel(col, row);
                        let pw = self.layout.cell_width;
                        let ph = self.layout.cell_height;

                        all_cells.push((px, py, pw, ph, bg));
                    }
                }
            }
        }

        autoreleasepool(|| {
            let layer = self.layer;
            let drawable: *mut objc::runtime::Object = unsafe { msg_send![layer, nextDrawable] };
            if drawable.is_null() {
                return;
            }
            let texture: *mut objc::runtime::Object = unsafe { msg_send![drawable, texture] };

            // Get drawable size for clip space transform
            let ds: core_graphics::geometry::CGSize = unsafe { msg_send![layer, drawableSize] };
            if ds.width <= 0.0 || ds.height <= 0.0 {
                return;
            }
            let screen_w = ds.width as f32;
            let screen_h = ds.height as f32;

            let desc = metal::RenderPassDescriptor::new();
            {
                let color_attach = desc.color_attachments().object_at(0).unwrap();
                let tex_ref = unsafe { metal::TextureRef::from_ptr(texture as *mut _) };
                color_attach.set_texture(Some(tex_ref));
                color_attach.set_load_action(metal::MTLLoadAction::Clear);
                color_attach.set_clear_color(metal::MTLClearColor::new(0.08, 0.08, 0.12, 1.0));
                color_attach.set_store_action(metal::MTLStoreAction::Store);
            }

            // Clamp to buffer capacity
            let max_cells = (self.cols * self.rows) as usize;
            let cell_count = all_cells.len().min(max_cells);
            let vertex_count = (cell_count * 6) as u64;

            if cell_count > 0 {
                self.build_quads(&all_cells[..cell_count], screen_w, screen_h);
            }

            let command_buffer = self.command_queue.new_command_buffer();
            let encoder = command_buffer.new_render_command_encoder(desc);
            encoder.set_render_pipeline_state(&self.bg_pipeline);
            encoder.set_vertex_buffer(0, Some(&self.vertex_buffer), 0);
            encoder.set_fragment_buffer(0, Some(&self.color_buffer), 0);
            encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, vertex_count);
            encoder.end_encoding();

            let draw_ref = unsafe { metal::MetalDrawableRef::from_ptr(drawable as *mut _) };
            command_buffer.present_drawable(draw_ref);
            command_buffer.commit();
        });
    }

    /// Build quad positions + colors into the vertex and color buffers.
    fn build_quads(&self, cells: &[(f64, f64, f64, f64, [f32; 4])], screen_w: f32, screen_h: f32) {
        let vbuf = self.vertex_buffer.contents() as *mut QuadVertex;
        let cbuf = self.color_buffer.contents() as *mut CellColor;

        for (i, (px, py, pw, ph, bg)) in cells.iter().enumerate() {
            // Convert pixel coords to clip space [-1, 1]
            // Metal: origin is top-left in pixels, bottom-left in clip space
            let x = *px as f32;
            let y = screen_h - *py as f32;
            let w = *pw as f32;
            let h = *ph as f32;

            // Convert to NDC: map [0, screen] → [-1, 1]
            let x0 = 2.0 * x / screen_w - 1.0;
            let y0 = 2.0 * y / screen_h - 1.0;
            let x1 = 2.0 * (x + w) / screen_w - 1.0;
            let y1 = 2.0 * (y - h) / screen_h - 1.0;

            // Two triangles (6 vertices) per quad
            let verts: [[f32; 2]; 6] = [
                // Triangle 1
                [x0, y0],
                [x1, y0],
                [x0, y1],
                // Triangle 2
                [x1, y0],
                [x1, y1],
                [x0, y1],
            ];

            let uvs: [[f32; 2]; 6] = [
                [0.0, 0.0],
                [1.0, 0.0],
                [0.0, 1.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.0, 1.0],
            ];

            unsafe {
                for v in 0..6 {
                    let idx = i * 6 + v;
                    *vbuf.add(idx) = QuadVertex {
                        position: verts[v],
                        texcoord: uvs[v],
                    };
                }
                *cbuf.add(i) = CellColor {
                    fg: [1.0, 1.0, 1.0, 1.0],
                    bg: *bg,
                };
            }
        }
    }
}
