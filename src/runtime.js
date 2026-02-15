// Runtime bootstrap - sets up console and gpu globals.
// Loaded as the extension's ESM entry point.

const { core } = Deno;

// --- Console API ---
function argsToMessage(...args) {
  return args.map((arg) => JSON.stringify(arg)).join(" ");
}

globalThis.console = {
  log: (...args) => {
    core.print(`[out]: ${argsToMessage(...args)}\n`, false);
  },
  error: (...args) => {
    core.print(`[err]: ${argsToMessage(...args)}\n`, true);
  },
};

// --- GPU Callbacks (invoked from Rust during the event loop) ---
globalThis.__gpuCallbacks = {
  setup: null,
  draw: null,
  resize: null,
};

// --- GPU API ---
globalThis.gpu = {
  createWindow: (title, width, height) => {
    core.ops.op_gpu_create_window(title, width, height);
  },

  onSetup: (fn) => {
    globalThis.__gpuCallbacks.setup = fn;
  },

  onDraw: (fn) => {
    globalThis.__gpuCallbacks.draw = fn;
  },

  onResize: (fn) => {
    globalThis.__gpuCallbacks.resize = fn;
  },

  createShaderModule: (code) => {
    return core.ops.op_gpu_create_shader_module(code);
  },

  createRenderPipeline: (shaderModuleId, vertexEntry, fragmentEntry) => {
    return core.ops.op_gpu_create_render_pipeline(
      shaderModuleId,
      vertexEntry,
      fragmentEntry,
    );
  },

  drawFrame: (pipelineId, r, g, b, a, vertexCount, instanceCount) => {
    core.ops.op_gpu_draw_frame(
      pipelineId,
      r,
      g,
      b,
      a,
      vertexCount,
      instanceCount,
    );
  },
};
