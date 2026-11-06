// Draws the scene with WebGL2 (draw-the-catalogues, navigate-with-the-mouse): one point pass,
// blended additively so that no depth sorting is needed, and one line pass for the outlines.
// Each batch is placed relative to the eye: its centre's offset from the eye is worked out in
// double precision once a frame, and its points' small offsets are float32.

import type { View } from "./camera.ts";
import { drawList } from "./drawList.ts";
import { Fades } from "./fades.ts";
import { type Sight, type Sphere, sightOf } from "./frustum.ts";
import type { Drawer, Lines, Rgba } from "./drawing.ts";
import { KIND_COLOURS, MARKER_INDEX, STRUCTURE_COLOUR, pointOpacity, structureStyle } from "./kinds.ts";
import { type Masks, NO_MASKS } from "./masks.ts";
import { projection } from "./projection.ts";
import { LINE_FRAGMENT, LINE_VERTEX, POINT_FRAGMENT, POINT_VERTEX } from "./shaders.ts";
import type { Mat3, Vec3 } from "./vec.ts";

/** The browser cannot draw with WebGL2. */
export class NoWebGL extends Error {}

/** A tile's points to draw: offsets from `centre`, each point's kind of value, and the sphere
 * they lie in, to leave them undrawn when it is out of view. */
export interface Points {
  id: string;
  /** The scale domain of the tile, whose sources `sources` index. */
  domain: string;
  centre: Vec3;
  offsets: Float32Array;
  kinds: Uint8Array;
  /** Each point's source, by its index in its domain's manifest. */
  sources: Uint16Array;
  /** Each point's own colour as RGBA bytes, for a star; none for a point drawn by its kind. */
  colours?: Uint8Array | undefined;
  extent: Sphere;
}

/** The diameter of a point, in device pixels. */
const POINT_SIZE = 2;

interface Drawn {
  vao: WebGLVertexArrayObject;
  buffers: WebGLBuffer[];
  centre: Vec3;
  count: number;
}

interface Outline extends Drawn {
  colour: Rgba;
}

/** The eye's axes, place and projection for one frame. */
interface Frame {
  axes: Mat3;
  eye: Vec3;
  projection: Float32Array;
}

/** A linked program and the locations of its uniforms. */
interface Program {
  program: WebGLProgram;
  axes: WebGLUniformLocation | null;
  centre: WebGLUniformLocation | null;
  projection: WebGLUniformLocation | null;
}

function attach(gl: WebGL2RenderingContext, program: WebGLProgram, type: number, source: string): void {
  const shader = gl.createShader(type);
  if (shader === null) throw new NoWebGL("WebGL2 gave no shader");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  gl.attachShader(program, shader);
}

function compile(gl: WebGL2RenderingContext, vertex: string, fragment: string): Program {
  const program = gl.createProgram();
  attach(gl, program, gl.VERTEX_SHADER, vertex);
  attach(gl, program, gl.FRAGMENT_SHADER, fragment);
  gl.linkProgram(program);
  if (gl.getProgramParameter(program, gl.LINK_STATUS) !== true) {
    throw new NoWebGL(gl.getProgramInfoLog(program) ?? "the shaders did not link");
  }
  const at = (name: string) => gl.getUniformLocation(program, name);
  return { program, axes: at("u_axes"), centre: at("u_centre"), projection: at("u_projection") };
}

export class Renderer implements Drawer {
  private readonly gl: WebGL2RenderingContext;
  private readonly points: Program;
  private readonly lines: Program;
  private readonly lineColour: WebGLUniformLocation | null;
  /** Each tile's points, by its id, with the sphere they lie in. */
  private readonly batches = new Map<string, Drawn & { id: string; extent: Points["extent"]; domain: string }>();
  /** How opaque each tile is as it fades in and out. */
  private readonly fades = new Fades();
  /** The share of each tile's points drawn, by its id. */
  private share: (id: string) => number = () => 1;
  /** What is turned off (choose-what-is-drawn). */
  private masks: Masks = NO_MASKS;
  private readonly hiddenKinds: WebGLUniformLocation | null;
  private readonly hiddenSources: WebGLUniformLocation | null;
  /** The outlines and markers not drawn, by name (choose-what-is-drawn). */
  private hiddenNames: ReadonlySet<string> = new Set();
  private readonly outlines = new Map<string, Outline>();
  private readonly markers = new Map<string, Drawn>();
  /** The name of the outline and markers drawn as chosen, if any. */
  private chosen: string | undefined;
  private readonly pointSize: WebGLUniformLocation | null;
  /** The markers' slot of the table of colours, set for each structure's markers as drawn. */
  private readonly markerColour: WebGLUniformLocation | null;
  private readonly opacity: WebGLUniformLocation | null;

  constructor(canvas: HTMLCanvasElement) {
    // No multisampling: the points soften their own edges, and samples multiply blending's cost.
    const gl = canvas.getContext("webgl2", { antialias: false, alpha: false });
    if (gl === null) throw new NoWebGL("WebGL2 is not available");
    this.gl = gl;
    this.points = compile(gl, POINT_VERTEX, POINT_FRAGMENT);
    this.lines = compile(gl, LINE_VERTEX, LINE_FRAGMENT);
    this.lineColour = gl.getUniformLocation(this.lines.program, "u_colour");
    gl.useProgram(this.points.program);
    this.pointSize = gl.getUniformLocation(this.points.program, "u_size");
    this.opacity = gl.getUniformLocation(this.points.program, "u_opacity");
    this.hiddenKinds = gl.getUniformLocation(this.points.program, "u_hiddenKinds");
    this.hiddenSources = gl.getUniformLocation(this.points.program, "u_hiddenSources");
    // A batch without colours of its own leaves `a_colour` at this constant: no colour, so that
    // its points take their kind's (WebGL's default would be opaque black).
    gl.vertexAttrib4f(gl.getAttribLocation(this.points.program, "a_colour"), 0, 0, 0, 0);
    // Nor sources: `a_source` beyond every mask's bits, so that nothing turned off hides them.
    gl.vertexAttrib1f(gl.getAttribLocation(this.points.program, "a_source"), 65535);
    const colours = new Float32Array([...KIND_COLOURS, STRUCTURE_COLOUR].flat());
    gl.uniform4fv(gl.getUniformLocation(this.points.program, "u_colours"), colours);
    this.markerColour = gl.getUniformLocation(this.points.program, `u_colours[${MARKER_INDEX}]`);
  }

  private buffer(
    program: WebGLProgram,
    name: string,
    data: Float32Array | Uint8Array | Uint16Array,
    size: number,
    normalized = false,
  ): WebGLBuffer {
    const gl = this.gl;
    const buffer = gl.createBuffer();
    const location = gl.getAttribLocation(program, name);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(location);
    const type = data instanceof Uint8Array ? gl.UNSIGNED_BYTE : data instanceof Uint16Array ? gl.UNSIGNED_SHORT : gl.FLOAT;
    gl.vertexAttribPointer(location, size, type, normalized, 0, 0);
    return buffer;
  }

  private release(drawn: Drawn): void {
    this.gl.deleteVertexArray(drawn.vao);
    for (const buffer of drawn.buffers) this.gl.deleteBuffer(buffer);
  }

  private upload(points: Pick<Points, "centre" | "offsets" | "kinds" | "colours"> & { sources?: Uint16Array }): Drawn {
    const vao = this.gl.createVertexArray();
    this.gl.bindVertexArray(vao);
    const buffers = [
      this.buffer(this.points.program, "a_offset", points.offsets, 3),
      this.buffer(this.points.program, "a_kind", points.kinds, 1),
    ];
    if (points.sources !== undefined) buffers.push(this.buffer(this.points.program, "a_source", points.sources, 1));
    if (points.colours !== undefined) buffers.push(this.buffer(this.points.program, "a_colour", points.colours, 4, true));
    this.gl.bindVertexArray(null);
    return { vao, buffers, centre: points.centre, count: points.kinds.length };
  }

  /** Adds a tile's points to what is drawn, fading them in from `now`. */
  addPoints(points: Points, now = performance.now()): void {
    this.forget(points.id);
    this.batches.set(points.id, { ...this.upload(points), id: points.id, extent: points.extent, domain: points.domain });
    this.fades.arrive(points.id, now);
  }

  /** Draws the points of the tile `id` no more, once they have faded out from `now`. */
  removePoints(id: string, now = performance.now()): void {
    if (this.batches.has(id)) this.fades.leave(id, now);
  }

  /** Lets go of the points of the tile `id` at once. */
  private forget(id: string): void {
    const drawn = this.batches.get(id);
    if (drawn === undefined) return;
    this.release(drawn);
    this.batches.delete(id);
  }

  /** Draws the share `share` gives of each tile's points, by its id. */
  setShare(share: (id: string) => number): void {
    this.share = share;
  }

  /** Whether a tile is fading in or out at `now`, so that frames are drawn until none is. */
  fading(now = performance.now()): boolean {
    return this.fades.moving(now);
  }

  /** Draws only the points `masks` leave on. */
  setHidden(masks: Masks): void {
    this.masks = masks;
  }

  /** Draws none of the outlines and markers named in `names`. */
  hideNames(names: ReadonlySet<string>): void {
    this.hiddenNames = names;
  }

  /** Draws the outline and markers named `name` as chosen, and none other; none for undefined. */
  choose(name: string | undefined): void {
    this.chosen = name;
  }

  /** Draws markers at `offsets` from `centre` under `name`, in place of what was drawn under it. */
  setMarkers(name: string, markers: { centre: Vec3; offsets: Float32Array } | undefined): void {
    const old = this.markers.get(name);
    if (old !== undefined) this.release(old);
    this.markers.delete(name);
    if (markers === undefined) return;
    const kinds = new Uint8Array(markers.offsets.length / 3).fill(MARKER_INDEX);
    this.markers.set(name, this.upload({ ...markers, kinds }));
  }

  /** Draws `lines` under `name`, in place of what was drawn under it; nothing for none. */
  setLines(name: string, lines: Lines | undefined): void {
    const old = this.outlines.get(name);
    if (old !== undefined) this.release(old);
    this.outlines.delete(name);
    if (lines === undefined) return;
    const vao = this.gl.createVertexArray();
    this.gl.bindVertexArray(vao);
    const buffers = [this.buffer(this.lines.program, "a_offset", lines.offsets, 3)];
    this.gl.bindVertexArray(null);
    const count = lines.offsets.length / 3;
    this.outlines.set(name, { vao, buffers, centre: lines.centre, count, colour: lines.colour });
  }

  /** Draws the view on a canvas `width` × `height` device pixels. */
  draw(view: View, toFrame: Mat3, width: number, height: number): void {
    const gl = this.gl;
    gl.viewport(0, 0, width, height);
    gl.clearColor(0.02, 0.027, 0.05, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
    const sight = sightOf(view, toFrame, width, height);
    const frame = { axes: sight.axes, eye: sight.eye, projection: projection(view, width / height) };
    this.drawOutlines(frame);
    this.drawPoints(frame, sight, view.distance);
    this.drawMarkers(frame);
  }

  private drawOutlines(frame: Frame): void {
    this.use(this.lines, frame);
    for (const [name, outline] of this.outlines) {
      if (this.hiddenNames.has(name)) continue;
      this.gl.uniform4f(this.lineColour, ...structureStyle(outline.colour, name === this.chosen).colour);
      this.drawOne(this.lines, frame, outline, this.gl.LINES);
    }
  }

  /** The tiles' points in sight, but those turned off: of each its share, as opaque as its fade. */
  private drawPoints(frame: Frame, sight: Sight, distance: number): void {
    const gl = this.gl;
    const now = performance.now();
    for (const id of this.fades.gone(now)) this.forget(id);
    this.use(this.points, frame);
    gl.uniform1f(this.pointSize, POINT_SIZE);
    gl.uniform1ui(this.hiddenKinds, this.masks.kinds);
    const opacity = pointOpacity(distance);
    const fade = (id: string) => this.fades.opacity(id, now);
    for (const { batch, count, fade: faded } of drawList(this.batches.values(), { sight, masks: this.masks, share: this.share, fade })) {
      gl.uniform1f(this.opacity, opacity * faded);
      gl.uniform1ui(this.hiddenSources, this.masks.sources.get(batch.domain) ?? 0);
      this.drawOne(this.points, frame, batch, gl.POINTS, count);
    }
  }

  /** The structures' markers, of the kind no mask holds and of no source. */
  private drawMarkers(frame: Frame): void {
    const gl = this.gl;
    gl.uniform1f(this.opacity, 1);
    for (const [name, marker] of this.markers) {
      if (this.hiddenNames.has(name)) continue;
      const style = structureStyle(STRUCTURE_COLOUR, name === this.chosen);
      gl.uniform1f(this.pointSize, style.markerSize);
      gl.uniform4f(this.markerColour, ...style.colour);
      this.drawOne(this.points, frame, marker, gl.POINTS);
    }
  }

  private use(program: Program, frame: { axes: Mat3; projection: Float32Array }): void {
    const gl = this.gl;
    gl.useProgram(program.program);
    gl.uniformMatrix4fv(program.projection, false, frame.projection);
    gl.uniformMatrix3fv(program.axes, true, frame.axes.flat());
  }

  /** Draws the first `count` of `drawn`, its centre placed from the eye in double precision. */
  private drawOne(program: Program, frame: { axes: Mat3; eye: Vec3 }, drawn: Drawn, mode: number, count = drawn.count): void {
    const { axes, eye } = frame;
    const dx = drawn.centre[0] - eye[0];
    const dy = drawn.centre[1] - eye[1];
    const dz = drawn.centre[2] - eye[2];
    const row = (r: Vec3) => r[0] * dx + r[1] * dy + r[2] * dz;
    this.gl.uniform3f(program.centre, row(axes[0]), row(axes[1]), row(axes[2]));
    this.gl.bindVertexArray(drawn.vao);
    this.gl.drawArrays(mode, 0, count);
  }

  /** Lets go of everything drawn, as when the view closes. */
  dispose(): void {
    for (const drawn of [...this.batches.values(), ...this.outlines.values(), ...this.markers.values()]) this.release(drawn);
    this.batches.clear();
    this.fades.clear();
    this.outlines.clear();
    this.markers.clear();
  }

  /** Gives the context back to the browser, which keeps only so many: once its canvas is gone
   * for good, since a canvas mounted again gets the same context, lost. */
  loseContext(): void {
    this.gl.getExtension("WEBGL_lose_context")?.loseContext();
  }
}
