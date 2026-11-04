// The projection matrix, column-major for WebGL.

import { FOV, type View } from "./camera.ts";

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
