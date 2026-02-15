mod gpu_ops;
mod gpu_state;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use deno_ast::MediaType;
use deno_ast::ParseParams;
use deno_core::extension;
use deno_core::ModuleLoadResponse;
use deno_core::ModuleSourceCode;
use deno_error::JsErrorBox;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{WindowAttributes, WindowId};

use crate::gpu_state::{GpuState, WindowConfig};

// ---------- TypeScript module loader (from blog post pt.2) ----------

struct TsModuleLoader;

impl deno_core::ModuleLoader for TsModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_core::ResolutionKind,
    ) -> Result<deno_core::ModuleSpecifier, JsErrorBox> {
        deno_core::resolve_import(specifier, referrer).map_err(JsErrorBox::from_err)
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&deno_core::ModuleLoadReferrer>,
        _options: deno_core::ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        let module_specifier = module_specifier.clone();

        let module_load = move || -> Result<deno_core::ModuleSource, JsErrorBox> {
            let path = module_specifier
                .to_file_path()
                .map_err(|_| JsErrorBox::generic("Cannot convert module specifier to file path"))?;

            let media_type = MediaType::from_path(&path);
            let (module_type, should_transpile) = match media_type {
                MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                    (deno_core::ModuleType::JavaScript, false)
                }
                MediaType::Jsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::TypeScript
                | MediaType::Mts
                | MediaType::Cts
                | MediaType::Dts
                | MediaType::Dmts
                | MediaType::Dcts
                | MediaType::Tsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::Json => (deno_core::ModuleType::Json, false),
                _ => {
                    return Err(JsErrorBox::generic(format!(
                        "Unknown extension {:?}",
                        path.extension()
                    )));
                }
            };

            let code = std::fs::read_to_string(&path).map_err(JsErrorBox::from_err)?;
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.clone(),
                    text: code.into(),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })
                .map_err(JsErrorBox::from_err)?;
                parsed
                    .transpile(
                        &Default::default(),
                        &Default::default(),
                        &Default::default(),
                    )
                    .map_err(JsErrorBox::from_err)?
                    .into_source()
                    .text
                    .into_bytes()
            } else {
                code.into_bytes()
            };

            let module = deno_core::ModuleSource::new(
                module_type,
                ModuleSourceCode::Bytes(code.into_boxed_slice().into()),
                &module_specifier,
                None,
            );
            Ok(module)
        };

        ModuleLoadResponse::Sync(module_load())
    }
}

// ---------- deno_core extension ----------

extension!(
    gpu_runtime,
    ops = [
        gpu_ops::op_gpu_create_window,
        gpu_ops::op_gpu_create_shader_module,
        gpu_ops::op_gpu_create_render_pipeline,
        gpu_ops::op_gpu_draw_frame,
    ],
    esm_entry_point = "ext:gpu_runtime/runtime.js",
    esm = [dir "src", "runtime.js"],
);

// ---------- Winit Application ----------

struct App {
    js_runtime: deno_core::JsRuntime,
    gpu_state: Rc<RefCell<Option<GpuState>>>,
    window_config: WindowConfig,
    setup_done: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.setup_done {
            return;
        }

        // Create the window
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title(&self.window_config.title)
                        .with_inner_size(PhysicalSize::new(
                            self.window_config.width,
                            self.window_config.height,
                        )),
                )
                .expect("Failed to create window"),
        );

        // Initialize wgpu
        let gpu = GpuState::new(window.clone());

        // Store GPU state (shared with ops)
        *self.gpu_state.borrow_mut() = Some(gpu);

        // Call JS setup callback
        let result = self
            .js_runtime
            .execute_script("<setup>", "globalThis.__gpuCallbacks.setup?.()");
        if let Err(e) = result {
            eprintln!("JS setup error: {}", e);
            event_loop.exit();
            return;
        }

        self.setup_done = true;
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                {
                    let mut gpu_opt = self.gpu_state.borrow_mut();
                    if let Some(gpu) = gpu_opt.as_mut() {
                        gpu.resize(size.width, size.height);
                    }
                }

                // Call JS resize callback
                let script = format!(
                    "globalThis.__gpuCallbacks.resize?.({}, {})",
                    size.width, size.height
                );
                let _ = self.js_runtime.execute_script("<resize>", script);
            }

            WindowEvent::RedrawRequested => {
                // Call JS draw callback
                let result = self
                    .js_runtime
                    .execute_script("<draw>", "globalThis.__gpuCallbacks.draw?.()");
                if let Err(e) = result {
                    eprintln!("JS draw error: {}", e);
                    event_loop.exit();
                    return;
                }

                // Request next frame
                let gpu_opt = self.gpu_state.borrow();
                if let Some(gpu) = gpu_opt.as_ref() {
                    gpu.window.request_redraw();
                }
            }

            _ => {}
        }
    }
}

// ---------- Main ----------

fn main() {
    // Get script path from CLI args
    let args: Vec<String> = std::env::args().collect();
    let file_path = match args.get(1) {
        Some(path) => path,
        None => {
            eprintln!("Usage: custom-js-runtime <script.ts>");
            std::process::exit(1);
        }
    };

    // Shared GPU state: None until resumed() initializes wgpu
    let gpu_state: Rc<RefCell<Option<GpuState>>> = Rc::new(RefCell::new(None));
    let gpu_state_for_ops = gpu_state.clone();

    // Create the JS runtime and load the user script.
    // The script registers callbacks and window config, then returns.
    let tokio_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let (js_runtime, window_config) = tokio_runtime.block_on(async {
        let mut js_runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
            module_loader: Some(Rc::new(TsModuleLoader)),
            extensions: vec![gpu_runtime::init()],
            ..Default::default()
        });

        // Inject shared GPU state into op_state
        js_runtime
            .op_state()
            .borrow_mut()
            .put::<Rc<RefCell<Option<GpuState>>>>(gpu_state_for_ops);

        // Load and evaluate the user script
        let main_module =
            deno_core::resolve_path(file_path, &std::env::current_dir().unwrap()).unwrap();
        let mod_id = js_runtime
            .load_main_es_module(&main_module)
            .await
            .expect("Failed to load module");
        let result = js_runtime.mod_evaluate(mod_id);
        js_runtime
            .run_event_loop(Default::default())
            .await
            .expect("Event loop error");
        result.await.expect("Module evaluation error");

        // Read window config (set by gpu.createWindow in JS)
        let window_config = js_runtime
            .op_state()
            .borrow()
            .try_borrow::<WindowConfig>()
            .cloned()
            .unwrap_or_default();

        (js_runtime, window_config)
    });

    // Create the event loop and run
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App {
        js_runtime,
        gpu_state,
        window_config,
        setup_done: false,
    };

    event_loop.run_app(&mut app).expect("Event loop failed");
}
