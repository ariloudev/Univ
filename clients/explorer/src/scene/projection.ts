// The projection matrix, column-major for WebGL, and the eye-relative placement of a batch.

import { FOV, type View, eyeAxes, eyeOf } from "./camera.ts";
import { type Mat3, type Vec3, apply, sub } from "./vec.ts";

/** A perspective projection for the view, near and far set by the distance to the focus. */
export function projection(view: View, aspect: number): Float32Array {
  const near = view.distance * 1e-4;
  const far = view.distance * 10 + 5e4;
  const f = 1 / Math.tan(FOV / 2);
  return new Float32Array([
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, (far + near) / (near - far), -1,
    0, 0, (2 * far * near) / (near - far), 0,
  ]);
}

/** The rows of the rotation into the eye's axes, flattened. */
export function axesOf(view: View, toFrame: Mat3): Float32Array {
  return new Float32Array(eyeAxes(view, toFrame).flat());
}

/** Where a batch centred at `centre` (ICRS megaparsecs) lies from the eye, in the eye's axes. */
export function centreFromEye(view: View, toFrame: Mat3, centre: Vec3): Float32Array {
  return new Float32Array(apply(eyeAxes(view, toFrame), sub(centre, eyeOf(view, toFrame))));
}
