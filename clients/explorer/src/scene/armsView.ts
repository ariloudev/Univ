// The arms view (see-the-galaxys-arms-face-on): the Galactic disk seen face-on from above the
// north Galactic pole, the focus halfway between the Sun and the Galactic Centre, near enough
// that every model arm and its extrapolation fills the view.

import { FOV, type View } from "./camera.ts";
import { toMpc } from "./units.ts";
import { type Mat3, unapply } from "./vec.ts";

/** The Galactic Centre's distance from the Sun that the model arms assume, kpc: R0 of Reid et
 * al. 2019 ("The models assume R0 = 8.15 kpc", Table 2's comments), as their structure files
 * cite it. */
export const GALACTIC_CENTRE_KPC = 8.15;

/** The focus's distance from the Sun toward l = 0°, b = 0°, kpc: halfway to the Galactic Centre. */
export const ARMS_FOCUS_KPC = GALACTIC_CENTRE_KPC / 2;

/** The farthest any point of a model arm or its extrapolation lies from the focus, kpc: an
 * extrapolation ends within 16 kpc of the Galactic Centre (record-a-researched-structure), and
 * the focus lies `ARMS_FOCUS_KPC` from it. */
export const ARMS_REACH_KPC = 16 + ARMS_FOCUS_KPC;

/** How much room the view leaves about the farthest arm point. */
const MARGIN = 1.05;

const MPC_PER_KPC = toMpc("kiloparsec") ?? 1e-3;

/** The arms view on a viewport `aspect` wide for each unit high, `toGalactic` turning the ICRS
 * into galactic axes: the eye above the north Galactic pole looking toward the south one, at yaw
 * 0, so that l = 90° is to the right and l = 180° up, the Sun above the Galactic Centre and the
 * Galaxy turning clockwise, as in Figure 1 of Reid et al. 2019; the eye far enough that a circle
 * of `ARMS_REACH_KPC` about the focus fits the narrower side of the view. */
export function armsView(toGalactic: Mat3, aspect: number): View {
  const focus = unapply(toGalactic, [ARMS_FOCUS_KPC * MPC_PER_KPC, 0, 0]);
  const half = Math.tan(FOV / 2) * Math.min(1, aspect);
  return { focus, distance: (ARMS_REACH_KPC * MPC_PER_KPC * MARGIN) / half, yaw: 0, pitch: Math.PI / 2 };
}
