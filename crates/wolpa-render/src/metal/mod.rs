//! ## Metal renderer
//!
//! Converts grid state into Metal draw commands and submits them to the GPU.
//!
//! ### Pipeline
//!
//! 1. Background pass — fill each cell with its background color.
//! 2. Glyph pass — sample glyph atlas, tint with foreground color.
//! 3. Cursor pass — render cursor block/underline.

use foreign_types::ForeignTypeRef;
use objc::rc::autoreleasepool;
use objc::{msg_send, sel, sel_impl};

use crate::font::CellMetrics;
use crate::layout::Layout;

/// Metal render state: device, shaders, pipeline, command queue, layer object.
pub struct MetalRenderer {
    pub device: metal::Device,
    pub command_queue: metal::CommandQueue,
    pub pipeline: metal::RenderPipelineState,
    pub vertex_buffer: metal::Buffer,
    /// Raw pointer to the CAMetalLayer. Retained by the Swift NSView.
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
    /// Create a Metal rendering context from a raw CAMetalLayer pointer.
    ///
    /// # Safety
    ///
    /// `layer_ptr` must be a valid, retained `CAMetalLayer *` that outlives
    /// the renderer. The layer must have a valid MTLDevice set.
    pub unsafe fn from_raw_layer(layer_ptr: *mut std::ffi::c_void, cols: u64, rows: u64) -> Self {
        let layer = layer_ptr as *mut objc::runtime::Object;
        // Get the device from the layer
        let _layer_device: *mut objc::runtime::Object = msg_send![layer, device];
        // Wrap as metal::Device. metal::Device::from_raw is used internally.
        // We'll create the device from system default — the same GPU supports both.
        let device = metal::Device::system_default().expect("no Metal GPU");

        let command_queue = device.new_command_queue();

        let options = metal::CompileOptions::new();
        let library = device
            .new_library_with_source(SHADER_SOURCE, &options)
            .expect("failed to compile Metal shaders");

        let vertex_func = library
            .get_function("vertex_main", None)
            .expect("vertex_main");
        let fragment_func = library
            .get_function("fragment_main", None)
            .expect("fragment_main");

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
            .expect("pipeline state");

        let num_cells = (cols * rows) as usize;
        let vbuf_size = (num_cells * 4 * std::mem::size_of::<QuadVertex>()) as u64;
        let vertex_buffer =
            device.new_buffer(vbuf_size, metal::MTLResourceOptions::StorageModeShared);

        let dummy_metrics = CellMetrics {
            width: 8.0,
            height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
        };
        let layout = Layout::new(cols, rows, &dummy_metrics);

        // Set layer pixel format via msg_send
        let _: () = msg_send![layer, setPixelFormat: metal::MTLPixelFormat::BGRA8Unorm as u64];

        MetalRenderer {
            device,
            command_queue,
            pipeline,
            vertex_buffer,
            layer,
            layout,
            cols,
            rows,
        }
    }

    /// Render one frame to the CAMetalLayer's next drawable.
    pub fn render_frame(&self) {
        autoreleasepool(|| {
            let layer = self.layer;
            // Get next drawable from the CAMetalLayer via msg_send
            let drawable: *mut objc::runtime::Object = unsafe { msg_send![layer, nextDrawable] };
            if drawable.is_null() {
                return;
            }
            let texture: *mut objc::runtime::Object = unsafe { msg_send![drawable, texture] };

            let desc = metal::RenderPassDescriptor::new();
            {
                let color_attach = desc.color_attachments().object_at(0).unwrap();
                // SAFETY: texture is a valid MTLTexture from CAMetalDrawable
                let tex_ref = unsafe { metal::TextureRef::from_ptr(texture as *mut _) };
                color_attach.set_texture(Some(tex_ref));
                color_attach.set_load_action(metal::MTLLoadAction::Clear);
                color_attach.set_clear_color(metal::MTLClearColor::new(0.1, 0.1, 0.15, 1.0));
                color_attach.set_store_action(metal::MTLStoreAction::Store);
            }

            let command_buffer = self.command_queue.new_command_buffer();
            let encoder = command_buffer.new_render_command_encoder(desc);
            encoder.set_render_pipeline_state(&self.pipeline);
            encoder.end_encoding();

            // SAFETY: drawable is a valid CAMetalDrawable
            let draw_ref = unsafe { metal::MetalDrawableRef::from_ptr(drawable as *mut _) };
            command_buffer.present_drawable(draw_ref);
            command_buffer.commit();
        });
    }

    /// Update the layout with new font metrics.
    pub fn update_layout(&mut self, metrics: &CellMetrics) {
        self.layout = Layout::new(self.cols, self.rows, metrics);
    }
}
