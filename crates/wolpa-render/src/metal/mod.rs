//! ## Metal renderer
//!
//! Converts grid state into Metal draw commands and submits them to the GPU.
//!
//! ### Pipeline
//!
//! 1. **Background pass** — Fill each cell with its background color.
//! 2. **Glyph pass** — Sample glyph atlas texture, tint with foreground color.
//! 3. **Cursor pass** — Render cursor block/underline.
//!
//! ### Shaders
//!
//! Vertex shader transforms cell quads to clip space. Fragment shader
//! samples the atlas texture alpha channel and applies per-cell colors.

use objc::rc::autoreleasepool;

use crate::font::CellMetrics;
use crate::layout::Layout;

/// Metal render state: device, shaders, pipeline, command queue.
pub struct MetalRenderer {
    pub device: metal::Device,
    pub command_queue: metal::CommandQueue,
    pub pipeline: metal::RenderPipelineState,
    pub vertex_buffer: metal::Buffer,
}

/// Vertex data for a single quad (cell): position (x, y) + UV (u, v).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct QuadVertex {
    pub position: [f32; 2],
    pub texcoord: [f32; 2],
}

/// Metal Shading Language source for the text renderer.
const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 uv;
};

struct CellUniforms {
    float4 fg_color;
    float4 bg_color;
};

vertex VertexOut vertex_main(
    uint vertexID [[vertex_id]],
    constant float2* positions [[buffer(0)]],
    constant float2* uvs [[buffer(1)]]
) {
    VertexOut out;
    out.position = float4(positions[vertexID], 0.0, 1.0);
    out.uv = uvs[vertexID];
    return out;
}

fragment float4 fragment_main(
    VertexOut in [[stage_in]],
    constant CellUniforms& uniforms [[buffer(0)]]
) {
    return uniforms.bg_color;
}
"#;

impl MetalRenderer {
    /// Create a new Metal rendering context with shaders compiled.
    pub fn new() -> Self {
        let device = metal::Device::system_default().expect("no Metal-capable GPU found");

        let command_queue = device.new_command_queue();

        // Compile shaders
        let options = metal::CompileOptions::new();
        let library = device
            .new_library_with_source(SHADER_SOURCE, &options)
            .expect("failed to compile Metal shaders");

        let vertex_func = library
            .get_function("vertex_main", None)
            .expect("vertex_main not found");
        let fragment_func = library
            .get_function("fragment_main", None)
            .expect("fragment_main not found");

        // Pipeline state
        let pipeline_desc = metal::RenderPipelineDescriptor::new();
        pipeline_desc.set_vertex_function(Some(&vertex_func));
        pipeline_desc.set_fragment_function(Some(&fragment_func));
        pipeline_desc
            .color_attachments()
            .object_at(0)
            .unwrap()
            .set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);

        let pipeline = device
            .new_render_pipeline_state(&pipeline_desc)
            .expect("failed to create pipeline state");

        // Pre-allocate a large vertex buffer (enough for 80x24 quads)
        let num_cells = 80 * 24;
        let quad_verts: Vec<QuadVertex> = Vec::with_capacity(num_cells * 4);
        let _ = quad_verts; // unused for now
        let vbuf_size = (num_cells * 4 * std::mem::size_of::<QuadVertex>()) as u64;

        let vertex_buffer =
            device.new_buffer(vbuf_size, metal::MTLResourceOptions::StorageModeShared);

        MetalRenderer {
            device,
            command_queue,
            pipeline,
            vertex_buffer,
        }
    }
}

impl Default for MetalRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MetalRenderer {
    /// Render a single frame to the given drawable (CAMetalDrawable).
    ///
    /// `layout` provides the cell → pixel mapping.
    /// `cells` is the flattened grid of (row, col, char, hl_id) tuples.
    /// For now, renders a single color quad as a placeholder.
    pub fn render_frame(
        &self,
        drawable: &metal::MetalDrawable,
        _layout: &Layout,
        _metrics: &CellMetrics,
    ) {
        autoreleasepool(|| {
            let texture = drawable.texture();
            let desc = metal::RenderPassDescriptor::new();
            {
                let color_attach = desc.color_attachments().object_at(0).unwrap();
                color_attach.set_texture(Some(texture));
                color_attach.set_load_action(metal::MTLLoadAction::Clear);
                color_attach.set_clear_color(metal::MTLClearColor::new(0.1, 0.1, 0.15, 1.0));
                color_attach.set_store_action(metal::MTLStoreAction::Store);
            }

            let command_buffer = self.command_queue.new_command_buffer();
            let encoder = command_buffer.new_render_command_encoder(desc);
            encoder.set_render_pipeline_state(&self.pipeline);

            // For now: draw nothing, just clear to background color.
            // In full implementation: build quad buffer from grid cells.
            encoder.end_encoding();
            command_buffer.present_drawable(drawable);
            command_buffer.commit();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_renderer() {
        let renderer = MetalRenderer::new();
        assert!(!renderer.device.name().is_empty());
    }
}
