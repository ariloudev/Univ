// The dust map drawn (draw-the-dust-map): its grid as a 3D texture of half floats, a haze ray-
// marched through it behind everything else, and the uniforms with which the point shader dims
// each point by the dust between it and the eye.

import { DIMMING, type DustVolume, dustFrame } from "./dustGrid.ts";
import { HAZE_FRAGMENT, HAZE_VERTEX } from "./dustShaders.ts";
import { type Mat3, type Vec3, apply, norm } from "./vec.ts";

/** How strongly the dust column turns the haze opaque, for display. */
const HAZE = 1.2;
/** The haze's colour: the brown of dust lit by starlight. */
const HAZE_COLOUR: Vec3 = [0.62, 0.45, 0.32];

/** `value` as an IEEE half float's bits, rounded toward zero; the densities are small and
 * positive, so no infinity or NaN is made. */
export function halfOf(value: number): number {
  const f = new Float32Array([value]);
  const bits = new Uint32Array(f.buffer)[0] ?? 0;
  const sign = (bits >>> 16) & 0x8000;
  const exponent = ((bits >>> 23) & 0xff) - 127 + 15;
  const mantissa = bits & 0x7fffff;
  if (exponent >= 31) return sign | 0x7bff;
  if (exponent <= 0) return exponent < -10 ? sign : sign | ((mantissa | 0x800000) >> (14 - exponent));
  return sign | (exponent << 10) | (mantissa >> 13);
}

/** The eye, its axes and projection, as the renderer draws a frame with them. */
interface Frame {
  axes: Mat3;
  eye: Vec3;
  projection: Float32Array;
}

/** The dust uniforms of a program. */
type DustUniforms = Record<"dust" | "on" | "eye" | "axes", WebGLUniformLocation | null>;

const dustUniforms = (gl: WebGL2RenderingContext, program: WebGLProgram): DustUniforms => {
  const at = (name: string) => gl.getUniformLocation(program, name);
  return { dust: at("u_dust"), on: at("u_dustOn"), eye: at("u_dustEye"), axes: at("u_dustAxes") };
};

export class Dust {
  private readonly gl: WebGL2RenderingContext;
  private readonly haze: WebGLProgram;
  private readonly hazeAt: DustUniforms & Record<"projection" | "reach" | "haze" | "colour", WebGLUniformLocation | null>;
  private readonly pointsAt: DustUniforms & { dimming: WebGLUniformLocation | null };
  private readonly empty: WebGLVertexArrayObject;
  /** The dust drawn, if any: its grid, its texture and the rotation that places it. */
  private shown: { volume: DustVolume; texture: WebGLTexture; toGalactic: Mat3 } | undefined;

  constructor(gl: WebGL2RenderingContext, points: WebGLProgram, compile: (vertex: string, fragment: string) => WebGLProgram) {
    this.gl = gl;
    this.haze = compile(HAZE_VERTEX, HAZE_FRAGMENT);
    const at = (name: string) => gl.getUniformLocation(this.haze, name);
    this.hazeAt = { ...dustUniforms(gl, this.haze), projection: at("u_projection"), reach: at("u_reach"), haze: at("u_haze"), colour: at("u_hazeColour") };
    this.pointsAt = { ...dustUniforms(gl, points), dimming: gl.getUniformLocation(points, "u_dimming") };
    this.empty = gl.createVertexArray();
  }

  /** Draws `volume`, placed by `toGalactic` (the ICRS to galactic axes), or none. */
  set(volume: DustVolume | undefined, toGalactic: Mat3): void {
    const gl = this.gl;
    this.clear();
    if (volume === undefined) return;
    const texture = gl.createTexture();
    this.shown = { volume, texture, toGalactic };
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_3D, texture);
    const halves = Uint16Array.from(volume.densities, (d) => halfOf(d * 1000));
    const { n } = volume;
    gl.texImage3D(gl.TEXTURE_3D, 0, gl.R16F, n, n, n, 0, gl.RED, gl.HALF_FLOAT, halves);
    for (const [name, value] of [
      [gl.TEXTURE_MIN_FILTER, gl.LINEAR],
      [gl.TEXTURE_MAG_FILTER, gl.LINEAR],
      [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE],
      [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE],
      [gl.TEXTURE_WRAP_R, gl.CLAMP_TO_EDGE],
    ] as const)
      gl.texParameteri(gl.TEXTURE_3D, name, value);
  }

  /** Sets the dust uniforms of the program in use, `at` its locations, for `frame`; off where
   * there is no dust or `on` is false. */
  private use(at: DustUniforms, frame: Frame, on: boolean): void {
    const gl = this.gl;
    const shown = on ? this.shown : undefined;
    gl.uniform1f(at.on, shown === undefined ? 0 : 1);
    gl.uniform1i(at.dust, 0);
    if (shown === undefined) return;
    const placed = dustFrame(shown.volume, frame.eye, frame.axes, shown.toGalactic);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_3D, shown.texture);
    gl.uniform3f(at.eye, ...placed.eye);
    gl.uniformMatrix3fv(at.axes, true, placed.axes.flat());
  }

  /** Sets the point program's dust for `frame`; `on` false for what the dust never dims, such as
   * the structures' markers. The point program must be in use. */
  usePoints(frame: Frame, on = true): void {
    this.use(this.pointsAt, frame, on);
    this.gl.uniform1f(this.pointsAt.dimming, DIMMING);
  }

  /** Draws the haze over the cleared view, blended by its opacity, before anything else. */
  drawHaze(frame: Frame): void {
    const { gl, shown } = this;
    if (shown === undefined) return;
    const { volume, toGalactic } = shown;
    gl.useProgram(this.haze);
    this.use(this.hazeAt, frame, true);
    gl.uniformMatrix4fv(this.hazeAt.projection, false, frame.projection);
    // Far enough along any ray from the eye to leave the grid behind: to its centre and across.
    const centre = (volume.corner + volume.edge / 2) * 1e-6;
    const fromCentre = norm(apply(toGalactic, frame.eye).map((c) => c - centre) as unknown as Vec3);
    gl.uniform1f(this.hazeAt.reach, fromCentre + volume.edge * Math.sqrt(3) * 1e-6);
    gl.uniform1f(this.hazeAt.haze, HAZE);
    gl.uniform3f(this.hazeAt.colour, ...HAZE_COLOUR);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
  }

  /** Lets go of the dust map, as when the view closes. */
  clear(): void {
    if (this.shown !== undefined) this.gl.deleteTexture(this.shown.texture);
    this.shown = undefined;
  }
}
