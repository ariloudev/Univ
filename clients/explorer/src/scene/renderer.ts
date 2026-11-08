// Draws the scene with WebGL2 (draw-the-catalogues, navigate-with-the-mouse): one point pass,
// blended additively so that no depth sorting is needed, and one line pass for the outlines.
// Each batch is placed relative to the eye: its centre's offset from the eye is worked out in
// double precision once a frame, and its points' small offsets are float32.

import { type Detail, type DrawnDetail, EVERYTHING, EVERY_POINT } from "../catalogue/lod.ts";
import { type View, pixelsAt } from "./camera.ts";
import { type ToDraw, drawList } from "./drawList.ts";
import { Fades } from "./fades.ts";
import { type Sight, type Sphere, sightOf } from "./frustum.ts";
import { type Drawer, FADE, type FixedSymbol, type Lines, type Rgba } from "./drawing.ts";
import { CULLED_OPACITY, KIND_COLOURS, MARKER_INDEX, STRUCTURE_COLOUR, detailOpacity, structureStyle } from "./kinds.ts";
import { type Masks, NO_MASKS } from "./masks.ts";
import { projection } from "./projection.ts";
import { ribbonOf } from "./ribbon.ts";
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
  /** The tile's slice of its domain, its points dividing it evenly in their order. */
  slice: readonly [number, number];
}

/** The diameter of a point, in device pixels. */
const POINT_SIZE = 2;

interface Drawn {
  vao: WebGLVertexArrayObject;
  buffers: WebGLBuffer[];
  centre: Vec3;
  count: number;
  /** Which points lie inside the volume the view is culled to, where it is: the mask as last
   * uploaded, and its buffer. */
  inside?: { mask: Uint8Array; buffer: WebGLBuffer };
}

/** A tile's points as uploaded, with what drawing them reads. */
type Batch = Drawn & { id: string; extent: Points["extent"]; domain: string; slice: Points["slice"] };

interface Outline extends Drawn {
  colour: Rgba;
  /** 1 where its lines fade as they crowd on the view, else 0. */
  fades: number;
  /** Its symbol, drawn at a fixed size on the view about its centre, where it has one. */
  symbol: (Drawn & Omit<FixedSymbol, "offsets">) | undefined;
}

/** The eye's axes, place and projection for one frame. */
interface Frame {
  axes: Mat3;
  eye: Vec3;
  projection: Float32Array;
}

/** How far ahead of the eye `point` lies along its line of sight, which perspective divides by. */
function depthOf({ axes, eye }: Frame, point: Vec3): number {
  const [bx = 0, by = 0, bz = 0] = axes[2];
  return -(bx * (point[0] - eye[0]) + by * (point[1] - eye[1]) + bz * (point[2] - eye[2]));
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

/** 1 where `lines` fade as they crowd on the view, else 0. */
const fadesOf = (lines: Lines): number => (lines.neighbours === undefined ? 0 : 1);

export class Renderer implements Drawer {
  private readonly gl: WebGL2RenderingContext;
  private readonly points: Program;
  private readonly lines: Program;
  private readonly lineColour: WebGLUniformLocation | null;
  private readonly lineWidth: WebGLUniformLocation | null;
  private readonly linePixels: WebGLUniformLocation | null;
  private readonly lineScale: WebGLUniformLocation | null;
  private readonly lineFade: Record<"on" | "range", WebGLUniformLocation | null>;
  /** Each tile's points, by its id, with the sphere they lie in. */
  private readonly batches = new Map<string, Batch>();
  /** The domain whose detail the point program holds, while a frame is drawn, and how opaque its
   * points are drawn. */
  private detailOf: string | undefined;
  private domainOpacity = 1;
  /** How opaque each tile is as it fades in and out. */
  private readonly fades = new Fades();
  /** Level of detail as the catalogue gives it, once there is one. */
  private drawnDetail: () => DrawnDetail | undefined = () => undefined;
  private readonly detailAt: Record<"onsets" | "cumulative" | "slice" | "count", WebGLUniformLocation | null>;
  /** What is turned off (choose-what-is-drawn). */
  private masks: Masks = NO_MASKS;
  private readonly hiddenKinds: WebGLUniformLocation | null;
  private readonly hiddenSources: WebGLUniformLocation | null;
  /** The point program's `a_inside`. */
  private readonly insideAt: number;
  /** The outlines and markers not drawn, by name (choose-what-is-drawn). */
  private hiddenNames: ReadonlySet<string> = new Set();
  private readonly outlines = new Map<string, Outline>();
  private readonly markers = new Map<string, Drawn & { colour: Rgba }>();
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
    this.lineWidth = gl.getUniformLocation(this.lines.program, "u_width");
    this.linePixels = gl.getUniformLocation(this.lines.program, "u_pixels");
    this.lineScale = gl.getUniformLocation(this.lines.program, "u_scale");
    const line = (name: string) => gl.getUniformLocation(this.lines.program, name);
    this.lineFade = { on: line("u_fades"), range: line("u_fadeRange") };
    gl.useProgram(this.points.program);
    this.pointSize = gl.getUniformLocation(this.points.program, "u_size");
    this.opacity = gl.getUniformLocation(this.points.program, "u_opacity");
    this.hiddenKinds = gl.getUniformLocation(this.points.program, "u_hiddenKinds");
    this.hiddenSources = gl.getUniformLocation(this.points.program, "u_hiddenSources");
    const at = (name: string) => gl.getUniformLocation(this.points.program, name);
    this.detailAt = { onsets: at("u_onsets"), cumulative: at("u_cumulative"), slice: at("u_slice"), count: at("u_count") };
    // A batch without colours of its own leaves `a_colour` at this constant: no colour, so that
    // its points take their kind's (WebGL's default would be opaque black).
    gl.vertexAttrib4f(gl.getAttribLocation(this.points.program, "a_colour"), 0, 0, 0, 0);
    // Nor sources: `a_source` beyond every mask's bits, so that nothing turned off hides them.
    gl.vertexAttrib1f(gl.getAttribLocation(this.points.program, "a_source"), 65535);
    // And every point lies inside while the view is not culled, markers always.
    this.insideAt = gl.getAttribLocation(this.points.program, "a_inside");
    gl.vertexAttrib1f(this.insideAt, 1);
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
    if (drawn.inside !== undefined) this.gl.deleteBuffer(drawn.inside.buffer);
  }

  /** Draws only the points of `drawn` that `mask` marks, or every one where it is none:
   * uploaded when the catalogue's mask for it changes, and let go once there is none. */
  private useInside(drawn: Drawn, mask: Uint8Array | undefined): void {
    if (drawn.inside?.mask === mask) return;
    this.letGoInside(drawn);
    if (mask !== undefined) drawn.inside = { mask, buffer: this.uploadInside(drawn.vao, mask) };
  }

  /** Draws every point of `drawn` again, and lets go of its mask's buffer. */
  private letGoInside(drawn: Drawn): void {
    if (drawn.inside === undefined) return;
    const gl = this.gl;
    gl.bindVertexArray(drawn.vao);
    gl.disableVertexAttribArray(this.insideAt);
    gl.bindVertexArray(null);
    gl.deleteBuffer(drawn.inside.buffer);
    delete drawn.inside;
  }

  /** Uploads `mask` as the `a_inside` of the points `vao` draws: its buffer. */
  private uploadInside(vao: WebGLVertexArrayObject, mask: Uint8Array): WebGLBuffer {
    const gl = this.gl;
    const buffer = gl.createBuffer();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, mask, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(this.insideAt);
    gl.vertexAttribPointer(this.insideAt, 1, gl.UNSIGNED_BYTE, false, 0, 0);
    gl.bindVertexArray(null);
    return buffer;
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
    this.batches.set(points.id, { ...this.upload(points), id: points.id, extent: points.extent, domain: points.domain, slice: points.slice });
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

  /** Draws the points with the level of detail `drawnDetail` gives, every point where it gives none. */
  setDetail(drawnDetail: () => DrawnDetail | undefined): void {
    this.drawnDetail = drawnDetail;
  }

  /** Sets the uniforms that draw a domain's points as `detail` gives. */
  private useDetail(detail: Detail): void {
    const gl = this.gl;
    gl.uniform1fv(this.detailAt.onsets, detail.onsets);
    gl.uniform1fv(this.detailAt.cumulative, detail.cumulative);
  }

  /** Whether a tile is fading in or out, so that frames are drawn until none is and the last
   * shows every tile as it stays. */
  fading(): boolean {
    return this.fades.moving();
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

  /** Draws markers at `offsets` from `centre` under `name`, in `colour`, in place of what was
   * drawn under it. */
  setMarkers(name: string, markers: { centre: Vec3; offsets: Float32Array; colour: Rgba } | undefined): void {
    const old = this.markers.get(name);
    if (old !== undefined) this.release(old);
    this.markers.delete(name);
    if (markers === undefined) return;
    const kinds = new Uint8Array(markers.offsets.length / 3).fill(MARKER_INDEX);
    this.markers.set(name, { ...this.upload({ ...markers, kinds }), colour: markers.colour });
  }

  /** The segments `offsets` about `centre`, with their ends' `neighbours` where they fade, as a
   * ribbon to draw. */
  private uploadLines(centre: Vec3, offsets: Float32Array, neighbours?: Float32Array): Drawn {
    const ribbon = ribbonOf(offsets, neighbours);
    const vao = this.gl.createVertexArray();
    this.gl.bindVertexArray(vao);
    const buffers = [
      this.buffer(this.lines.program, "a_from", ribbon.from, 3),
      this.buffer(this.lines.program, "a_to", ribbon.to, 3),
      this.buffer(this.lines.program, "a_corner", ribbon.corners, 2),
    ];
    // Lines that do not fade leave a_neighbour unread: u_fades is 0 for them.
    if (ribbon.neighbours !== undefined) buffers.push(this.buffer(this.lines.program, "a_neighbour", ribbon.neighbours, 3));
    this.gl.bindVertexArray(null);
    return { vao, buffers, centre, count: ribbon.count };
  }

  /** Lets go of an outline and its symbol. */
  private releaseOutline(outline: Outline): void {
    this.release(outline);
    if (outline.symbol !== undefined) this.release(outline.symbol);
  }

  /** Draws `lines` under `name`, in place of what was drawn under it; nothing for none. */
  setLines(name: string, lines: Lines | undefined): void {
    const old = this.outlines.get(name);
    if (old !== undefined) this.releaseOutline(old);
    this.outlines.delete(name);
    if (lines === undefined) return;
    const { symbol: fixed } = lines;
    const symbol = fixed === undefined ? undefined : { ...this.uploadLines(lines.centre, fixed.offsets), pixels: fixed.pixels, maxWidth: fixed.maxWidth };
    this.outlines.set(name, { ...this.uploadLines(lines.centre, lines.offsets, lines.neighbours), colour: lines.colour, fades: fadesOf(lines), symbol });
  }

  /** Draws the view on a canvas `width` × `height` device pixels, `ratio` of them to a CSS pixel. */
  draw(view: View, toFrame: Mat3, width: number, height: number, ratio: number): void {
    const gl = this.gl;
    gl.viewport(0, 0, width, height);
    gl.clearColor(0.02, 0.027, 0.05, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
    const sight = sightOf(view, toFrame, width, height);
    const frame = { axes: sight.axes, eye: sight.eye, projection: projection(view, width / height) };
    this.drawOutlines(frame, width, height, ratio);
    this.drawPoints(frame, sight, view.distance);
    this.drawMarkers(frame);
  }

  private drawOutlines(frame: Frame, width: number, height: number, ratio: number): void {
    this.use(this.lines, frame);
    this.gl.uniform2f(this.linePixels, width / 2, height / 2);
    this.gl.uniform1f(this.lineScale, 1);
    // The fade's bounds are CSS pixels, so many device pixels each.
    this.gl.uniform2f(this.lineFade.range, FADE.gone * ratio, FADE.full * ratio);
    for (const [name, outline] of this.outlines) {
      if (this.hiddenNames.has(name)) continue;
      this.gl.uniform1f(this.lineFade.on, outline.fades);
      const style = structureStyle(outline.colour, name === this.chosen);
      this.gl.uniform4f(this.lineColour, ...style.colour);
      this.gl.uniform1f(this.lineWidth, style.lineWidth);
      if (outline.count > 0) this.drawOne(this.lines, frame, outline, this.gl.TRIANGLES);
      this.drawSymbol(frame, outline, style.lineWidth, height);
    }
  }

  /** An outline's symbol, if it has one, at its fixed size on a view `height` pixels high, in
   * lines no wider than `lineWidth` nor its own widest; the scale left at 1 for outlines. */
  private drawSymbol(frame: Frame, { symbol, centre }: Outline, lineWidth: number, height: number): void {
    if (symbol === undefined) return;
    this.gl.uniform1f(this.lineWidth, Math.min(lineWidth, symbol.maxWidth));
    this.gl.uniform1f(this.lineScale, pixelsAt(symbol.pixels, depthOf(frame, centre), height));
    this.drawOne(this.lines, frame, symbol, this.gl.TRIANGLES);
    this.gl.uniform1f(this.lineScale, 1);
  }

  /** The tiles' points in sight, but those turned off: of each its share, as opaque as its fade. */
  private drawPoints(frame: Frame, sight: Sight, distance: number): void {
    const gl = this.gl;
    const now = performance.now();
    for (const id of this.fades.gone(now)) this.forget(id);
    this.use(this.points, frame);
    gl.uniform1f(this.pointSize, POINT_SIZE);
    gl.uniform1ui(this.hiddenKinds, this.masks.kinds);
    const detail = this.drawnDetail() ?? EVERY_POINT;
    const fade = (id: string) => this.fades.opacity(id, now);
    const share = (id: string) => detail.share(id);
    this.detailOf = undefined;
    for (const drawn of drawList(this.batches.values(), { sight, masks: this.masks, share, fade })) {
      this.drawBatch(frame, drawn, distance, detail);
    }
    this.fades.drawn(now);
  }

  /** The first `count` points of `batch`, `fade` times `opacity` as opaque, each drawn where its
   * rank lies below the share its domain needs at its distance. */
  private drawBatch(frame: Frame, { batch, count, fade }: ToDraw<Batch>, distance: number, detail: DrawnDetail): void {
    const gl = this.gl;
    if (batch.domain !== this.detailOf) {
      this.detailOf = batch.domain;
      const domainDetail = detail.detail(batch.domain);
      this.useDetail(domainDetail);
      this.domainOpacity = detailOpacity(domainDetail, distance);
    }
    const inside = detail.inside(batch.id);
    this.useInside(batch, inside);
    gl.uniform2f(this.detailAt.slice, batch.slice[0], batch.slice[1]);
    gl.uniform1f(this.detailAt.count, batch.count);
    gl.uniform1f(this.opacity, (inside === undefined ? this.domainOpacity : CULLED_OPACITY) * fade);
    gl.uniform1ui(this.hiddenSources, this.masks.sources.get(batch.domain) ?? 0);
    this.drawOne(this.points, frame, batch, gl.POINTS, count);
  }

  /** The structures' markers, of the kind no mask holds and of no source. */
  private drawMarkers(frame: Frame): void {
    const gl = this.gl;
    gl.uniform1f(this.opacity, 1);
    // Every marker drawn: one level holding all, and a slice that ranks every point first.
    this.useDetail(EVERYTHING);
    gl.uniform2f(this.detailAt.slice, 0, 0);
    for (const [name, marker] of this.markers) {
      if (this.hiddenNames.has(name)) continue;
      const style = structureStyle(marker.colour, name === this.chosen);
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
    for (const drawn of [...this.batches.values(), ...this.markers.values()]) this.release(drawn);
    for (const outline of this.outlines.values()) this.releaseOutline(outline);
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
