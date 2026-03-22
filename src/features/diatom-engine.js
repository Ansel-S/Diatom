/**
 * diatom/src/features/diatom-engine.js  — v7
 *
 * Generative Diatom Engine.
 * Renders a unique, symmetric geometric glyph derived from the user's
 * weekly behaviour metrics.
 *
 * Primary path:  WebGPU compute → render into <canvas>
 * Fallback path: procedural SVG (pure JS, no canvas needed)
 *
 * Mapping rules (from spec):
 *   focus_score   → symmetry axes (3–12)
 *   density_score → branch complexity (fine detail)
 *   breadth_score → radial spread (arm length)
 *
 * NO external chart libraries are used.
 */

'use strict';

// ── WebGPU path ────────────────────────────────────────────────────────────────

const WGSL_SHADER = `
struct Uniforms {
  symmetry_axes    : u32,
  branch_complexity: f32,
  radial_spread    : f32,
  saturation       : f32,
  time             : f32,
};

@group(0) @binding(0) var<uniform> u : Uniforms;
@group(0) @binding(1) var out_tex : texture_storage_2d<rgba8unorm, write>;

const PI = 3.14159265;
const TAU = 6.28318530;

fn sdf_arm(r: f32, theta: f32, complexity: f32, spread: f32) -> f32 {
  // Petal SDF with sub-branches
  let base     = r - spread * 0.45 * (1.0 + 0.3 * cos(theta * (3.0 + complexity * 7.0)));
  let fringe   = 0.04 * complexity * cos(theta * (8.0 + complexity * 20.0));
  return base + fringe;
}

fn palette(t: f32, sat: f32) -> vec3f {
  // A → B → C (scholar blue, builder purple, leisure amber)
  let a = vec3f(0.37, 0.65, 0.98);  // scholar blue
  let b = vec3f(0.66, 0.55, 0.98);  // builder purple
  let c = vec3f(0.98, 0.57, 0.24);  // leisure amber
  let col = mix(mix(a, b, clamp(t * 2.0, 0.0, 1.0)),
                mix(b, c, clamp(t * 2.0 - 1.0, 0.0, 1.0)), step(0.5, t));
  return mix(vec3f(0.5), col, sat);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  let dim   = textureDimensions(out_tex);
  if (gid.x >= dim.x || gid.y >= dim.y) { return; }

  let uv    = (vec2f(f32(gid.x), f32(gid.y)) / vec2f(f32(dim.x), f32(dim.y))) * 2.0 - 1.0;
  let r     = length(uv);
  let angle = atan2(uv.y, uv.x);

  // N-fold symmetry
  let n       = f32(u.symmetry_axes);
  let sector  = floor((angle + PI) / (TAU / n));
  let folded  = (angle + PI) - sector * (TAU / n);
  // Mirror within sector
  let mirrored = abs(folded - PI / n);

  // Compute SDF for the arm
  let d = sdf_arm(r, mirrored, u.branch_complexity, u.radial_spread);

  // Inside the arm: bright glyph colour; outside: dark background
  let inside  = 1.0 - smoothstep(-0.02, 0.02, d);
  let ring    = 1.0 - smoothstep(0.0, 0.015, abs(d));  // silhouette edge glow

  // Colour by angle (maps to scholar/builder/leisure palette)
  let t       = (angle + PI) / TAU;
  let col     = palette(t, u.saturation);

  let pixel   = col * inside + col * ring * 0.4;
  let alpha   = inside + ring * 0.4;

  // Add breathing pulse
  let pulse   = 0.5 + 0.5 * sin(u.time * 0.8);
  let final_alpha = clamp(alpha * (0.85 + 0.15 * pulse), 0.0, 1.0);

  textureStore(out_tex, vec2i(i32(gid.x), i32(gid.y)), vec4f(pixel, final_alpha));
}
`;

// ── WebGPU renderer ────────────────────────────────────────────────────────────

async function renderDiatomWebGPU(canvas, params) {
  if (!navigator.gpu) throw new Error('WebGPU not available');

  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter)  throw new Error('no WebGPU adapter');
  const device   = await adapter.requestDevice();
  const format   = navigator.gpu.getPreferredCanvasFormat();

  const ctx = canvas.getContext('webgpu');
  ctx.configure({ device, format, alphaMode: 'premultiplied' });

  const SIZE  = canvas.width;                // 320
  const axes  = Math.round(3 + params.focus * 9);  // 3–12
  const sat   = 0.3 + params.focus * 0.7;

  // Uniform buffer: 5 x f32/u32 = 20 bytes → pad to 32
  const uniformData = new ArrayBuffer(32);
  const u32View     = new Uint32Array(uniformData);
  const f32View     = new Float32Array(uniformData);
  u32View[0] = axes;
  f32View[1] = params.density;
  f32View[2] = 0.3 + params.breadth * 0.6;
  f32View[3] = sat;
  f32View[4] = performance.now() / 1000;

  const uniformBuf = device.createBuffer({
    size: 32,
    usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
  });
  device.queue.writeBuffer(uniformBuf, 0, uniformData);

  // Storage texture
  const texture = device.createTexture({
    size: [SIZE, SIZE],
    format: 'rgba8unorm',
    usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC,
  });

  const shaderModule = device.createShaderModule({ code: WGSL_SHADER });
  const pipeline = device.createComputePipeline({
    layout: 'auto',
    compute: { module: shaderModule, entryPoint: 'main' },
  });

  const bindGroup = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: uniformBuf } },
      { binding: 1, resource: texture.createView() },
    ],
  });

  const encoder = device.createCommandEncoder();
  const pass    = encoder.beginComputePass();
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, bindGroup);
  pass.dispatchWorkgroups(Math.ceil(SIZE / 8), Math.ceil(SIZE / 8));
  pass.end();

  // Copy texture → canvas
  const renderPipeline = device.createRenderPipeline({
    layout: 'auto',
    vertex: {
      module: device.createShaderModule({ code: `
        @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
          var pos = array<vec2f,4>(vec2f(-1,-1),vec2f(1,-1),vec2f(-1,1),vec2f(1,1));
          return vec4f(pos[i], 0, 1);
        }
      `}),
      entryPoint: 'vs',
    },
    fragment: {
      module: device.createShaderModule({ code: `
        @group(0) @binding(0) var t: texture_2d<f32>;
        @group(0) @binding(1) var s: sampler;
        @fragment fn fs(@builtin(position) p: vec4f) -> @location(0) vec4f {
          let uv = p.xy / vec2f(${SIZE}.0, ${SIZE}.0);
          return textureSample(t, s, uv);
        }
      `}),
      entryPoint: 'fs',
      targets: [{ format }],
    },
    primitive: { topology: 'triangle-strip' },
  });

  const sampler    = device.createSampler({ magFilter: 'linear', minFilter: 'linear' });
  const renderBG   = device.createBindGroup({
    layout: renderPipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: texture.createView() },
      { binding: 1, resource: sampler },
    ],
  });

  const renderPass = encoder.beginRenderPass({
    colorAttachments: [{
      view: ctx.getCurrentTexture().createView(),
      clearValue: { r: 0.04, g: 0.04, b: 0.055, a: 1 },
      loadOp: 'clear',
      storeOp: 'store',
    }],
  });
  renderPass.setPipeline(renderPipeline);
  renderPass.setBindGroup(0, renderBG);
  renderPass.draw(4);
  renderPass.end();

  device.queue.submit([encoder.finish()]);
}

// ── SVG fallback renderer ──────────────────────────────────────────────────────
// Pure 2D canvas, zero external deps. Same visual concept, no GPU required.

export function renderDiatomSvg(canvas, params) {
  const ctx  = canvas.getContext('2d');
  const SIZE = canvas.width;
  const cx   = SIZE / 2;
  const cy   = SIZE / 2;
  const axes = Math.round(3 + params.focus * 9);    // 3–12
  const r    = cx * (0.3 + params.breadth * 0.55);  // arm length
  const sub  = Math.round(1 + params.density * 5);  // sub-branch count

  ctx.clearRect(0, 0, SIZE, SIZE);

  // Background
  ctx.fillStyle = 'rgba(10,10,16,1)';
  ctx.fillRect(0, 0, SIZE, SIZE);

  // Draw N-fold symmetric arms
  for (let i = 0; i < axes; i++) {
    const angle = (i / axes) * Math.PI * 2;
    drawArm(ctx, cx, cy, angle, r, sub, params, axes);
    // Mirror
    drawArm(ctx, cx, cy, angle + Math.PI / axes, r * 0.7, Math.max(1, sub - 1), params, axes);
  }

  // Silhouette outline ring
  const sat  = 0.3 + params.focus * 0.7;
  ctx.beginPath();
  ctx.arc(cx, cy, r * 0.96, 0, Math.PI * 2);
  ctx.strokeStyle = `hsla(230,${Math.round(sat * 70)}%,70%,.12)`;
  ctx.lineWidth = 1;
  ctx.stroke();

  // Centre glow
  const grd = ctx.createRadialGradient(cx, cy, 0, cx, cy, r * 0.3);
  grd.addColorStop(0, `hsla(250,70%,80%,${0.15 + params.focus * 0.1})`);
  grd.addColorStop(1, 'transparent');
  ctx.fillStyle = grd;
  ctx.beginPath();
  ctx.arc(cx, cy, r * 0.3, 0, Math.PI * 2);
  ctx.fill();
}

function drawArm(ctx, cx, cy, angle, r, sub, params, axes) {
  const step  = Math.PI / (axes * 4);
  const hue   = 210 + (angle / (Math.PI * 2)) * 140;  // 210 (blue) → 350 (amber)
  const sat   = 60 + params.density * 30;
  const light = 60 + params.focus * 15;

  ctx.beginPath();
  ctx.moveTo(cx, cy);

  for (let j = -sub; j <= sub; j++) {
    const a    = angle + j * step * (1 + params.density * 0.5);
    const dist = r * (1 - Math.abs(j) / (sub + 1) * 0.4);
    const x    = cx + Math.cos(a) * dist;
    const y    = cy + Math.sin(a) * dist;

    if (j === -sub) {
      ctx.moveTo(cx, cy);
      ctx.lineTo(x, y);
    } else {
      const px = cx + Math.cos(angle + (j - 1) * step * (1 + params.density * 0.5)) * r * (1 - Math.abs(j - 1) / (sub + 1) * 0.4);
      const py = cy + Math.sin(angle + (j - 1) * step * (1 + params.density * 0.5)) * r * (1 - Math.abs(j - 1) / (sub + 1) * 0.4);
      ctx.quadraticCurveTo((px + cx) / 2, (py + cy) / 2, x, y);
    }
  }

  const alpha = 0.4 + params.focus * 0.4;
  ctx.strokeStyle = `hsla(${hue},${sat}%,${light}%,${alpha})`;
  ctx.lineWidth   = 1 + params.density * 1.5;
  ctx.lineCap     = 'round';
  ctx.stroke();
}

// ── Main export ───────────────────────────────────────────────────────────────

/**
 * Render the Generative Diatom into a canvas element.
 * Tries WebGPU first; falls back to SVG-style 2D canvas.
 *
 * @param {HTMLCanvasElement} canvas
 * @param {{ focus: number, breadth: number, density: number }} params  — all 0.0–1.0
 */
export async function renderDiatom(canvas, params) {
  try {
    await renderDiatomWebGPU(canvas, params);
  } catch {
    renderDiatomSvg(canvas, params);
  }
}
