const shaderCode = `
@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(in_vertex_index) - 1);
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
`;

let pipeline: number | null = null;

gpu.createWindow("Hello Triangle", 800, 600);

gpu.onSetup(() => {
  const shader: number = gpu.createShaderModule(shaderCode);
  pipeline = gpu.createRenderPipeline(shader, "vs_main", "fs_main");
  console.log("Pipeline created successfully");
});

gpu.onDraw(() => {
  if (pipeline !== null) {
    // Green background, draw 3 vertices (1 triangle)
    gpu.drawFrame(pipeline, 0.0, 1.0, 0.0, 1.0, 3, 1);
  }
});

gpu.onResize((width: number, height: number) => {
  console.log(`Resized to ${width}x${height}`);
});
