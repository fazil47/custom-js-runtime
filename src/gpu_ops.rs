use std::cell::RefCell;
use std::rc::Rc;

use deno_core::op2;
use deno_core::OpState;

use crate::gpu_state::{GpuState, WindowConfig};

type SharedGpuState = Rc<RefCell<Option<GpuState>>>;

/// Store window configuration. Called from JS before the event loop starts.
#[op2(fast)]
pub fn op_gpu_create_window(
    state: &mut OpState,
    #[string] title: String,
    #[smi] width: u32,
    #[smi] height: u32,
) {
    state.put(WindowConfig {
        title,
        width,
        height,
    });
}

/// Create a wgpu ShaderModule from WGSL source code. Returns a handle ID.
#[op2(fast)]
#[smi]
pub fn op_gpu_create_shader_module(state: &mut OpState, #[string] code: String) -> u32 {
    let gpu_state_rc = state.borrow::<SharedGpuState>().clone();
    let mut gpu_state_opt = gpu_state_rc.borrow_mut();
    let gpu = gpu_state_opt
        .as_mut()
        .expect("GPU not initialized - createShaderModule called before setup");

    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("User Shader"),
            source: wgpu::ShaderSource::Wgsl(code.into()),
        });

    let id = gpu.shader_modules.len() as u32;
    gpu.shader_modules.push(shader);
    id
}

/// Create a render pipeline using a shader module handle. Returns a pipeline handle ID.
#[op2(fast)]
#[smi]
pub fn op_gpu_create_render_pipeline(
    state: &mut OpState,
    #[smi] shader_module_id: u32,
    #[string] vertex_entry: String,
    #[string] fragment_entry: String,
) -> u32 {
    let gpu_state_rc = state.borrow::<SharedGpuState>().clone();
    let mut gpu_state_opt = gpu_state_rc.borrow_mut();
    let gpu = gpu_state_opt
        .as_mut()
        .expect("GPU not initialized - createRenderPipeline called before setup");

    let shader = &gpu.shader_modules[shader_module_id as usize];
    let surface_format = gpu.config.format;

    let pipeline_layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[],
            ..Default::default()
        });

    let pipeline = gpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(&vertex_entry),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(&fragment_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

    let id = gpu.render_pipelines.len() as u32;
    gpu.render_pipelines.push(pipeline);
    id
}

/// Execute a full render frame: get surface texture, create encoder,
/// begin render pass with clear color, set pipeline, draw, submit, present.
#[op2(fast)]
pub fn op_gpu_draw_frame(
    state: &mut OpState,
    #[smi] pipeline_id: u32,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    #[smi] vertex_count: u32,
    #[smi] instance_count: u32,
) {
    let gpu_state_rc = state.borrow::<SharedGpuState>().clone();
    let mut gpu_state_opt = gpu_state_rc.borrow_mut();
    let gpu = gpu_state_opt
        .as_mut()
        .expect("GPU not initialized - drawFrame called before setup");

    let surface_texture = match gpu.surface.get_current_texture() {
        Ok(tex) => tex,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            gpu.surface.configure(&gpu.device, &gpu.config);
            gpu.surface
                .get_current_texture()
                .expect("Failed to acquire surface texture after reconfigure")
        }
        Err(e) => {
            eprintln!("Surface error: {:?}", e);
            return;
        }
    };

    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Frame Encoder"),
        });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r,
                        g,
                        b,
                        a,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        let pipeline = &gpu.render_pipelines[pipeline_id as usize];
        render_pass.set_pipeline(pipeline);
        render_pass.draw(0..vertex_count, 0..instance_count);
    }

    gpu.queue.submit(std::iter::once(encoder.finish()));
    surface_texture.present();
}
