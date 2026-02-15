use std::sync::Arc;

/// Configuration for the window, set from JS before the event loop starts.
#[derive(Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Custom JS Runtime".to_string(),
            width: 800,
            height: 600,
        }
    }
}

/// Holds all wgpu resources. Created during `resumed()` and shared
/// with deno_core ops via `Rc<RefCell<Option<GpuState>>>`.
pub struct GpuState {
    pub window: Arc<winit::window::Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub shader_modules: Vec<wgpu::ShaderModule>,
    pub render_pipelines: Vec<wgpu::RenderPipeline>,
}

impl GpuState {
    /// Initialize wgpu with the given window. Blocks on async adapter/device requests.
    pub fn new(window: Arc<winit::window::Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("Failed to find a suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("GPU Device"),
                ..Default::default()
            },
        ))
        .expect("Failed to create GPU device");

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Surface is not supported by the adapter");
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            shader_modules: Vec::new(),
            render_pipelines: Vec::new(),
        }
    }

    /// Reconfigure the surface after a resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }
}
