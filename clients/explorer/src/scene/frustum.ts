// What the eye sees (draw-the-catalogues): the view's frustum, tested against bounding spheres,
// and the focal length that turns sizes into pixels. The frustum has no far plane: everything
// in front of the eye within the field of view is in view.

import { FOV, type View, eyeAxes, eyeOf, focalPixels } from "./camera.ts";
import { type Mat3, type Vec3, apply, sub } from "./vec.ts";

/** The eye of a view on a viewport, and what it sees. */
export interface Sight {
  /** The eye, ICRS megaparsecs. */
  eye: Vec3;
  /** The focal length, pixels. */
  focal: number;
  /** Whether any part of the sphere about `centre` of `radius` (megaparsecs) is in view. */
  inView(centre: Vec3, radius: number): boolean;
}

/** The items of `items` in view: those whose sphere about `centre` of `radius` reaches into it. */
export function inSight<T extends { centre: Vec3; radius: number }>(items: Iterable<T>, sight: Sight): T[] {
  return [...items].filter((item) => sight.inView(item.centre, item.radius));
}

/** The sight of `view` on a viewport `width` × `height` pixels. */
export function sightOf(view: View, toFrame: Mat3, width: number, height: number): Sight {
  const axes = eyeAxes(view, toFrame);
  const eye = eyeOf(view, toFrame);
  const tanY = Math.tan(FOV / 2);
  const tanX = tanY * (width / Math.max(1, height));
  // Each side plane's normal, as the cosine and sine of its half-angle.
  const side = (tan: number) => ({ cos: 1 / Math.sqrt(1 + tan * tan), sin: tan / Math.sqrt(1 + tan * tan) });
  const x = side(tanX);
  const y = side(tanY);
  return {
    eye,
    focal: focalPixels(height),
    inView(centre, radius) {
      const local = apply(axes, sub(centre, eye));
      const depth = -local[2];
      if (depth < -radius) return false;
      // The sphere's distance outside each side plane, which passes through the eye.
      return Math.abs(local[0]) * x.cos - depth * x.sin <= radius && Math.abs(local[1]) * y.cos - depth * y.sin <= radius;
    },
  };
}
