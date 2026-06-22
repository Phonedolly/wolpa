//! ## Metal renderer
//!
//! Dual-pass: background then foreground with glyph atlas sampling.

use foreign_types::ForeignTypeRef;
use objc::rc::autoreleasepool;
use objc::{msg_send, sel, sel_impl};

use crate::atlas::{GlyphAtlas, GlyphUV};
use crate::font::{CellMetrics, Font};
use crate::layout::Layout;
use wolpa_core::grid::GridState;
use wolpa_core::highlight::HighlightResolver;

pub struct MetalRenderer {
    pub device: metal::Device,
    pub command_queue: metal::CommandQueue,
    pub bg_pipeline: metal::RenderPipelineState,
    pub fg_pipeline: metal::RenderPipelineState,
    pub pos_buffer: metal::Buffer,
    pub uv_buffer: metal::Buffer,
    pub idx_buffer: metal::Buffer,
    pub color_buffer: metal::Buffer,
    pub atlas: GlyphAtlas,
    layer: *mut objc::runtime::Object,
    pub layout: Layout,
    pub cols: u64,
    pub rows: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CellColor {
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

type BgCell = (f64, f64, f64, f64, [f32; 4], [f32; 4]);
type FgCell = (f64, f64, f64, f64, [f32; 4], GlyphUV);

const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 uv;
    uint cell_idx;
};

struct CellColor {
    float4 fg;
    float4 bg;
};

// ── Background ──────────────────────────────────────────────────

vertex VertexOut bg_vertex(
    uint vertexID [[vertex_id]],
    constant float2* positions [[buffer(0)]],
    constant float2* uvs [[buffer(1)]],
    constant uint* cell_indices [[buffer(2)]]
) {
    VertexOut out;
    out.position = float4(positions[vertexID], 0.0, 1.0);
    out.uv = uvs[vertexID];
    out.cell_idx = cell_indices[vertexID];
    return out;
}

fragment float4 bg_fragment(
    VertexOut in [[stage_in]],
    constant CellColor* colors [[buffer(0)]]
) {
    return colors[in.cell_idx].bg;
}

// ── Foreground ──────────────────────────────────────────────────

vertex VertexOut fg_vertex(
    uint vertexID [[vertex_id]],
    constant float2* positions [[buffer(0)]],
    constant float2* uvs [[buffer(1)]],
    constant uint* cell_indices [[buffer(2)]]
) {
    VertexOut out;
    out.position = float4(positions[vertexID], 0.0, 1.0);
    out.uv = uvs[vertexID];
    out.cell_idx = cell_indices[vertexID];
    return out;
}

fragment float4 fg_fragment(
    VertexOut in [[stage_in]],
    constant CellColor* colors [[buffer(0)]],
    texture2d<float> atlas [[texture(0)]]
) {
    constexpr sampler s(filter::linear);
    float alpha = atlas.sample(s, in.uv).r;
    float4 fg = colors[in.cell_idx].fg;
    return float4(fg.rgb, alpha);
}
"#;

impl MetalRenderer {
    /// # Safety
    ///
    /// `layer_ptr` must be a valid, retained `CAMetalLayer *`.
    pub unsafe fn from_raw_layer(layer_ptr: *mut std::ffi::c_void, cols: u64, rows: u64) -> Self {
        let layer = layer_ptr as *mut objc::runtime::Object;
        let _: () = msg_send![layer, setPixelFormat: metal::MTLPixelFormat::BGRA8Unorm as u64];
        let device = metal::Device::system_default().expect("no Metal GPU");
        let command_queue = device.new_command_queue();

        let options = metal::CompileOptions::new();
        let library = device
            .new_library_with_source(SHADER_SOURCE, &options)
            .expect("Metal shaders");

        let bg_vertex = library.get_function("bg_vertex", None).expect("bg_vertex");
        let bg_fragment = library
            .get_function("bg_fragment", None)
            .expect("bg_fragment");
        let fg_vertex = library.get_function("fg_vertex", None).expect("fg_vertex");
        let fg_fragment = library
            .get_function("fg_fragment", None)
            .expect("fg_fragment");

        let bg_desc = metal::RenderPipelineDescriptor::new();
        bg_desc.set_vertex_function(Some(&bg_vertex));
        bg_desc.set_fragment_function(Some(&bg_fragment));
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_blending_enabled(true);
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_rgb_blend_operation(metal::MTLBlendOperation::Add);
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_alpha_blend_operation(metal::MTLBlendOperation::Add);
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
        bg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

        let fg_desc = metal::RenderPipelineDescriptor::new();
        fg_desc.set_vertex_function(Some(&fg_vertex));
        fg_desc.set_fragment_function(Some(&fg_fragment));
        fg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
        fg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_blending_enabled(true);
        fg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_rgb_blend_operation(metal::MTLBlendOperation::Add);
        fg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_alpha_blend_operation(metal::MTLBlendOperation::Add);
        fg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
        fg_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

        let bg_pipeline = device
            .new_render_pipeline_state(&bg_desc)
            .expect("bg pipeline");
        let fg_pipeline = device
            .new_render_pipeline_state(&fg_desc)
            .expect("fg pipeline");

        let num_cells = (cols * rows) as usize;
        let vcount = num_cells * 6;
        let pbuf_size = (vcount * 8) as u64; // [f32; 2] = 8 bytes
        let ibuf_size = (vcount * 4) as u64; // u32 = 4 bytes
        let cbuf_size = (num_cells * 32) as u64; // CellColor = 32 bytes

        let pos_buffer = device.new_buffer(pbuf_size, metal::MTLResourceOptions::StorageModeShared);
        let uv_buffer = device.new_buffer(pbuf_size, metal::MTLResourceOptions::StorageModeShared);
        let idx_buffer = device.new_buffer(ibuf_size, metal::MTLResourceOptions::StorageModeShared);
        let color_buffer =
            device.new_buffer(cbuf_size, metal::MTLResourceOptions::StorageModeShared);

        let atlas = GlyphAtlas::new(&device, 2048);

        let dummy_metrics = CellMetrics {
            width: 8.0,
            height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
        };
        let layout = Layout::new(cols, rows, &dummy_metrics);

        MetalRenderer {
            device,
            command_queue,
            bg_pipeline,
            fg_pipeline,
            pos_buffer,
            uv_buffer,
            idx_buffer,
            color_buffer,
            atlas,
            layer,
            layout,
            cols,
            rows,
        }
    }

    pub fn render_grid(
        &mut self,
        grid: &GridState,
        highlight: &HighlightResolver,
        metrics: &CellMetrics,
        font: &Font,
    ) {
        self.layout = Layout::new(self.cols, self.rows, metrics);

        // Collect cells
        let mut bg_cells = Vec::new();
        let mut fg_cells = Vec::new();

        let target_grid = grid.grid(2).or_else(|| grid.grid(1));
        if let Some(g) = target_grid {
            // Cursor position
            let cur_row = g.cursor_row;
            let cur_col = g.cursor_col;

            for row in 0..g.height {
                for col in 0..g.width {
                    let cell = g.cell(row, col);
                    let attr = highlight.resolve(cell.hl_id);
                    let mut bg = attr.background.unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    let fg = attr.foreground.unwrap_or([1.0, 1.0, 1.0, 1.0]);

                    // Highlight cursor cell
                    if row == cur_row && col == cur_col {
                        bg = [0.4, 0.5, 0.6, 1.0];
                    }

                    let (px, py) = self.layout.cell_to_pixel(col, row);
                    let pw = self.layout.cell_width;
                    let ph = self.layout.cell_height;

                    bg_cells.push((px, py, pw, ph, bg, fg));

                    let ch = cell.text.chars().next().unwrap_or(' ');
                    if ch != ' ' {
                        // Rasterize glyph if needed
                        let (pixels, w, h) = font.rasterize_glyph(ch);
                        if let Some(uv) =
                            self.atlas
                                .get_or_upload(font.font_size as u64, ch, &pixels, w, h)
                        {
                            fg_cells.push((px, py, pw, ph, fg, uv));
                        }
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
            let ds: core_graphics::geometry::CGSize = unsafe { msg_send![layer, drawableSize] };
            if ds.width <= 0.0 || ds.height <= 0.0 {
                return;
            }
            let screen_w = ds.width as f32;
            let screen_h = ds.height as f32;

            let texture: *mut objc::runtime::Object = unsafe { msg_send![drawable, texture] };

            let desc = metal::RenderPassDescriptor::new();
            {
                let ca = desc.color_attachments().object_at(0).unwrap();
                let tex_ref = unsafe { metal::TextureRef::from_ptr(texture as *mut _) };
                ca.set_texture(Some(tex_ref));
                ca.set_load_action(metal::MTLLoadAction::Clear);
                ca.set_clear_color(metal::MTLClearColor::new(0.078, 0.078, 0.118, 1.0));
                ca.set_store_action(metal::MTLStoreAction::Store);
            }

            let command_buffer = self.command_queue.new_command_buffer();
            let encoder = command_buffer.new_render_command_encoder(desc);

            // ── Background pass ──
            let max_cells = (self.cols * self.rows) as usize;
            let bg_count = bg_cells.len().min(max_cells);
            if bg_count > 0 {
                self.build_bg_quads(&bg_cells[..bg_count], screen_w, screen_h);
                encoder.set_render_pipeline_state(&self.bg_pipeline);
                encoder.set_vertex_buffer(0, Some(&self.pos_buffer), 0);
                encoder.set_vertex_buffer(1, Some(&self.uv_buffer), 0);
                encoder.set_vertex_buffer(2, Some(&self.idx_buffer), 0);
                encoder.set_fragment_buffer(0, Some(&self.color_buffer), 0);
                encoder.draw_primitives(
                    metal::MTLPrimitiveType::Triangle,
                    0,
                    (bg_count * 6) as u64,
                );
            }

            // ── Foreground pass ──
            let fg_count = fg_cells.len().min(max_cells);
            if fg_count > 0 {
                self.build_fg_quads(&fg_cells[..fg_count], screen_w, screen_h);
                encoder.set_render_pipeline_state(&self.fg_pipeline);
                encoder.set_vertex_buffer(0, Some(&self.pos_buffer), 0);
                encoder.set_vertex_buffer(1, Some(&self.uv_buffer), 0);
                encoder.set_vertex_buffer(2, Some(&self.idx_buffer), 0);
                encoder.set_fragment_buffer(0, Some(&self.color_buffer), 0);
                encoder.set_fragment_texture(0, Some(&self.atlas.texture));
                encoder.draw_primitives(
                    metal::MTLPrimitiveType::Triangle,
                    0,
                    (fg_count * 6) as u64,
                );
            }

            encoder.end_encoding();
            let draw_ref = unsafe { metal::MetalDrawableRef::from_ptr(drawable as *mut _) };
            command_buffer.present_drawable(draw_ref);
            command_buffer.commit();
        });
    }

    fn build_bg_quads(&self, cells: &[BgCell], sw: f32, sh: f32) {
        let pbuf = self.pos_buffer.contents() as *mut [f32; 2];
        let ubuf = self.uv_buffer.contents() as *mut [f32; 2];
        let ibuf = self.idx_buffer.contents() as *mut u32;
        let cbuf = self.color_buffer.contents() as *mut CellColor;

        for (i, &(px, py, pw, ph, bg, fg)) in cells.iter().enumerate() {
            let (verts, uvs) = quad_verts(px, py, pw, ph, sw, sh);
            unsafe {
                for v in 0..6 {
                    let idx = i * 6 + v;
                    *pbuf.add(idx) = verts[v];
                    *ubuf.add(idx) = uvs[v];
                    *ibuf.add(idx) = i as u32;
                }
                *cbuf.add(i) = CellColor { fg, bg };
            }
        }
    }

    fn build_fg_quads(&self, cells: &[FgCell], sw: f32, sh: f32) {
        let pbuf = self.pos_buffer.contents() as *mut [f32; 2];
        let ubuf = self.uv_buffer.contents() as *mut [f32; 2];
        let ibuf = self.idx_buffer.contents() as *mut u32;
        let cbuf = self.color_buffer.contents() as *mut CellColor;

        for (i, &(px, py, pw, ph, fg, uv)) in cells.iter().enumerate() {
            // Use glyph UVs (atlas coordinates) instead of [0,1]
            let glyph_uvs: [[f32; 2]; 6] = [
                [uv.u0, uv.v0],
                [uv.u1, uv.v0],
                [uv.u0, uv.v1],
                [uv.u1, uv.v0],
                [uv.u1, uv.v1],
                [uv.u0, uv.v1],
            ];
            let (verts, _) = quad_verts(px, py, pw, ph, sw, sh);
            unsafe {
                for v in 0..6 {
                    let idx = i * 6 + v;
                    *pbuf.add(idx) = verts[v];
                    *ubuf.add(idx) = glyph_uvs[v];
                    *ibuf.add(idx) = i as u32;
                }
                *cbuf.add(i) = CellColor {
                    fg,
                    bg: [0.0, 0.0, 0.0, 0.0],
                };
            }
        }
    }
}

fn quad_verts(
    px: f64,
    py: f64,
    pw: f64,
    ph: f64,
    sw: f32,
    sh: f32,
) -> ([[f32; 2]; 6], [[f32; 2]; 6]) {
    let x = px as f32;
    let y = sh - py as f32;
    let w = pw as f32;
    let h = ph as f32;
    let x0 = 2.0 * x / sw - 1.0;
    let y0 = 2.0 * y / sh - 1.0;
    let x1 = 2.0 * (x + w) / sw - 1.0;
    let y1 = 2.0 * (y - h) / sh - 1.0;

    let verts: [[f32; 2]; 6] = [[x0, y0], [x1, y0], [x0, y1], [x1, y0], [x1, y1], [x0, y1]];
    let uvs: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [1.0, 0.0],
        [0.0, 1.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];
    (verts, uvs)
}
