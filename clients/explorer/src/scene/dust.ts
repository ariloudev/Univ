// The dust map drawn (draw-the-dust-map): its grid as a 3D texture of half floats, a haze ray-
// marched through it behind everything else, and the uniforms with which the point shader dims
// each point by the dust between it and the eye.

import { type DustFrame, type DustVolume, dustFrame } from "./dustGrid.ts";
import { HAZE_FRAGMENT, HAZE_VERTEX } from "./dustShaders.ts";
import { type Mat3, type Vec3, apply, norm, sub } from "./vec.ts";

/** The eye, its axes and projection, as the renderer draws a frame with them. */
interface Frame {
  axes: Mat3;
  eye: Vec3;
  projection: Float32Array;
}

/** The dust uniforms of a program. */
type DustUniforms = Record<"on" | "eye" | "axes", WebGLUniformLocation | null>;

const dustUniforms = (gl: WebGL2RenderingContext, program: WebGLProgram): DustUniforms => {
  const at = (name: string) => gl.getUniformLocation(program, name);
  return { on: at("u_dustOn"), eye: at("u_dustEye"), axes: at("u_dustAxes") };
};

/** The dust drawn: its grid, its texture and the rotation that places it. */
interface Shown {
  volume: DustVolume;
  texture: WebGLTexture;
  toGalactic: Mat3;
}

export class Dust {
  private readonly gl: WebGL2RenderingContext;
  private readonly haze: WebGLProgram;
  private readonly hazeAt: DustUniforms & Record<"projection" | "reach", WebGLUniformLocation | null>;
  private readonly pointsAt: DustUniforms;
  private readonly empty: WebGLVertexArrayObject;
  private shown: Shown | undefined;
  /** Where the dust lies from the eye, for the frame being drawn. */
  private placed: DustFrame | undefined;

  constructor(gl: WebGL2RenderingContext, points: WebGLProgram, compile: (vertex: string, fragment: string) => WebGLProgram) {
    this.gl = gl;
    this.haze = compile(HAZE_VERTEX, HAZE_FRAGMENT);
    const at = (name: string) => gl.getUniformLocation(this.haze, name);
    this.hazeAt = { ...dustUniforms(gl, this.haze), projection: at("u_projection"), reach: at("u_reach") };
    this.pointsAt = dustUniforms(gl, points);
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
    // Rows of any width, and densities in thousandths, which half floats hold to about 0.1%:
    // WebGL turns the floats into half floats itself.
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    const { n } = volume;
    gl.texImage3D(gl.TEXTURE_3D, 0, gl.R16F, n, n, n, 0, gl.RED, gl.FLOAT, volume.densities.map((d) => d * 1000));
    for (const [name, value] of [
      [gl.TEXTURE_MIN_FILTER, gl.LINEAR],
      [gl.TEXTURE_MAG_FILTER, gl.LINEAR],
      [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE],
      [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE],
      [gl.TEXTURE_WRAP_R, gl.CLAMP_TO_EDGE],
    ] as const)
      gl.texParameteri(gl.TEXTURE_3D, name, value);
  }

  /** Sets the dust uniforms of the program in use, `at` its locations, for the frame placed. */
  private use(at: DustUniforms): void {
    const gl = this.gl;
    const { shown, placed } = this;
    gl.uniform1f(at.on, placed === undefined ? 0 : 1);
    if (shown === undefined || placed === undefined) return;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_3D, shown.texture);
    gl.uniform3f(at.eye, ...placed.eye);
    gl.uniformMatrix3fv(at.axes, true, placed.axes.flat());
  }

  /** Places the dust for `frame` and draws its haze over the cleared view, blended by its
   * opacity, before anything else: the blend function is left as it set it. */
  drawHaze(frame: Frame): void {
    const { gl, shown } = this;
    this.placed = shown === undefined ? undefined : dustFrame(shown.volume, frame.eye, frame.axes, shown.toGalactic);
    if (shown === undefined) return;
    const { volume, toGalactic } = shown;
    gl.useProgram(this.haze);
    this.use(this.hazeAt);
    gl.uniformMatrix4fv(this.hazeAt.projection, false, frame.projection);
    // Far enough along any ray from the eye to leave the grid behind: to its centre and across.
    const centre = (volume.corner + volume.edge / 2) * 1e-6;
    const fromCentre = norm(sub(apply(toGalactic, frame.eye), [centre, centre, centre]));
    gl.uniform1f(this.hazeAt.reach, fromCentre + volume.edge * Math.sqrt(3) * 1e-6);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** Sets the point program's dust for the frame `drawHaze` placed; the program must be in use. */
  usePoints(): void {
    this.use(this.pointsAt);
  }

  /** Lets go of the dust map, as when the view closes. */
  clear(): void {
    if (this.shown !== undefined) this.gl.deleteTexture(this.shown.texture);
    this.shown = undefined;
    this.placed = undefined;
  }
}
