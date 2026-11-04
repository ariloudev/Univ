// Draws the scene with WebGL2 (draw-the-catalogues, navigate-with-the-mouse): one point pass,
// blended additively so that no depth sorting is needed, and one line pass for the outlines.

import type { View } from "./camera.ts";
import { axesOf, centreFromEye, projection } from "./projection.ts";
import { LINE_FRAGMENT, LINE_VERTEX, POINT_FRAGMENT, POINT_VERTEX } from "./shaders.ts";
import type { Mat3, Vec3 } from "./vec.ts";

/** The browser cannot draw with WebGL2. */
export class NoWebGL extends Error {}

/** Points to draw: offsets from `centre` and each point's kind of value. */
export interface Points {
  centre: Vec3;
  offsets: Float32Array;
  kinds: Uint8Array;
}

/** Line segments to draw: pairs of offsets from `centre`, in one colour (RGBA, 0 to 1). */
export interface Lines {
  centre: Vec3;
  offsets: Float32Array;
  colour: readonly [number, number, number, number];
}

interface Drawn {
  vao: WebGLVertexArrayObject;
  centre: Vec3;
  count: number;
}

function attach(gl: WebGL2RenderingContext, program: WebGLProgram, type: number, source: string): void {
  const shader = gl.createShader(type);
  if (shader === null) throw new NoWebGL("WebGL2 gave no shader");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  gl.attachShader(program, shader);
}

function compile(gl: WebGL2RenderingContext, vertex: string, fragment: string): WebGLProgram {
  const program = gl.createProgram();
  attach(gl, program, gl.VERTEX_SHADER, vertex);
  attach(gl, program, gl.FRAGMENT_SHADER, fragment);
  gl.linkProgram(program);
  if (gl.getProgramParameter(program, gl.LINK_STATUS) !== true) {
    throw new NoWebGL(gl.getProgramInfoLog(program) ?? "the shaders did not link");
  }
  return program;
}

export class Renderer {
  private readonly gl: WebGL2RenderingContext;
  private readonly points: WebGLProgram;
  private readonly lines: WebGLProgram;
  private readonly batches: Drawn[] = [];
  private outlines = new Map<string, Drawn & { colour: Lines["colour"] }>();
  private colours = new Float32Array(16);
  pointSize = 2;

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
    if (gl === null) throw new NoWebGL("WebGL2 is not available");
    this.gl = gl;
    this.points = compile(gl, POINT_VERTEX, POINT_FRAGMENT);
    this.lines = compile(gl, LINE_VERTEX, LINE_FRAGMENT);
  }

  /** The colour of each kind of value, in the manifests' order of kinds. */
  setColours(colours: readonly (readonly [number, number, number, number])[]): void {
    this.colours = new Float32Array(colours.flat());
  }

  private buffer(program: WebGLProgram, name: string, data: Float32Array | Uint8Array, size: number): void {
    const gl = this.gl;
    const location = gl.getAttribLocation(program, name);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(location);
    const type = data instanceof Uint8Array ? gl.UNSIGNED_BYTE : gl.FLOAT;
    gl.vertexAttribPointer(location, size, type, false, 0, 0);
  }

  /** Adds a tile's points to what is drawn. */
  addPoints(points: Points): void {
    const vao = this.gl.createVertexArray();
    this.gl.bindVertexArray(vao);
    this.buffer(this.points, "a_offset", points.offsets, 3);
    this.buffer(this.points, "a_kind", points.kinds, 1);
    this.gl.bindVertexArray(null);
    this.batches.push({ vao, centre: points.centre, count: points.kinds.length });
  }

  /** Draws `lines` under `name`, in place of what was drawn under it; nothing for none. */
  setLines(name: string, lines: Lines | undefined): void {
    const old = this.outlines.get(name);
    if (old !== undefined) this.gl.deleteVertexArray(old.vao);
    this.outlines.delete(name);
    if (lines === undefined) return;
    const vao = this.gl.createVertexArray();
    this.gl.bindVertexArray(vao);
    this.buffer(this.lines, "a_offset", lines.offsets, 3);
    this.gl.bindVertexArray(null);
    this.outlines.set(name, { vao, centre: lines.centre, count: lines.offsets.length / 3, colour: lines.colour });
  }

  /** Draws the view on a canvas `width` × `height` device pixels. */
  draw(view: View, toFrame: Mat3, width: number, height: number): void {
    const gl = this.gl;
    gl.viewport(0, 0, width, height);
    gl.clearColor(0.02, 0.027, 0.05, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
    const projection_ = projection(view, width / height);
    const axes = axesOf(view, toFrame);
    this.pass(this.lines, projection_, axes, () => {
      for (const outline of this.outlines.values()) {
        gl.uniform4fv(gl.getUniformLocation(this.lines, "u_colour"), [...outline.colour]);
        this.drawOne(this.lines, view, toFrame, outline, gl.LINES);
      }
    });
    this.pass(this.points, projection_, axes, () => {
      gl.uniform1f(gl.getUniformLocation(this.points, "u_size"), this.pointSize);
      gl.uniform4fv(gl.getUniformLocation(this.points, "u_colours"), this.colours);
      for (const batch of this.batches) this.drawOne(this.points, view, toFrame, batch, gl.POINTS);
    });
  }

  private pass(program: WebGLProgram, projection_: Float32Array, axes: Float32Array, body: () => void): void {
    const gl = this.gl;
    gl.useProgram(program);
    gl.uniformMatrix4fv(gl.getUniformLocation(program, "u_projection"), false, projection_);
    gl.uniformMatrix3fv(gl.getUniformLocation(program, "u_axes"), true, axes);
    body();
  }

  private drawOne(program: WebGLProgram, view: View, toFrame: Mat3, drawn: Drawn, mode: number): void {
    const gl = this.gl;
    gl.uniform3fv(gl.getUniformLocation(program, "u_centre"), centreFromEye(view, toFrame, drawn.centre));
    gl.bindVertexArray(drawn.vao);
    gl.drawArrays(mode, 0, drawn.count);
  }

  /** Lets go of every point drawn, as when another release is chosen. */
  clearPoints(): void {
    for (const batch of this.batches) this.gl.deleteVertexArray(batch.vao);
    this.batches.length = 0;
  }
}
