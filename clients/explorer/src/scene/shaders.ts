// The renderer's shaders: points with a soft round edge, and lines; both placed relative to the
// eye, each batch by its centre's offset from the eye and its own small offsets.

import { LEVELS } from "../catalogue/lod.ts";
import { KINDS } from "./kinds.ts";

/** The colours of the kinds of value and, after them, of structures' markers. */
const COLOURS = KINDS.length + 1;

export const POINT_VERTEX = `#version 300 es
in vec3 a_offset;
in float a_kind;
in float a_source;
in vec4 a_colour;
// Whether the point lies inside the volume the view is culled to (see-the-structures): 1 for
// every point while it is not culled.
in float a_inside;
// The kinds of value, and the batch's sources by index, turned off (choose-what-is-drawn).
uniform uint u_hiddenKinds;
uniform uint u_hiddenSources;
uniform mat3 u_axes;
uniform vec3 u_centre;
uniform mat4 u_projection;
uniform float u_size;
uniform vec4 u_colours[${COLOURS}];
uniform float u_opacity;
// Level of detail (draw-the-catalogues), as lod.ts's neededAt and rankOf: for each level of the
// domain, the distance within which the view starts to need it (0, never, past the domain's
// depth) and the share the levels down to it hold; the batch's slice of its domain, and its points.
uniform float u_onsets[${LEVELS}];
uniform float u_cumulative[${LEVELS}];
uniform vec2 u_slice;
uniform float u_count;
out vec4 v_colour;
// Whether a point of this rank is needed at distance d from the eye: below the share of the
// domain the view needs there, the root's share at once and each level's in proportion.
bool needed(float rank, float d) {
  float share = u_cumulative[0];
  if (rank < share) return true;
  float near = 1.0 / max(d, 1e-12);
  for (int level = 1; level < ${LEVELS}; level++) {
    share += (u_cumulative[level] - u_cumulative[level - 1]) * clamp(u_onsets[level] * near - 1.0, 0.0, 1.0);
  }
  return rank < share;
}
void main() {
  uint kind = uint(a_kind);
  uint source = uint(a_source);
  vec3 place = u_axes * a_offset + u_centre;
  float rank = mix(u_slice.x, u_slice.y, (float(gl_VertexID) + 0.5) / max(u_count, 1.0));
  bool hidden = ((u_hiddenKinds >> kind) & 1u) != 0u || (source < 32u && ((u_hiddenSources >> source) & 1u) != 0u)
    || a_inside < 0.5 || !needed(rank, length(place));
  if (hidden) {
    // Outside the clip volume, and of no size: not drawn.
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    gl_PointSize = 0.0;
    v_colour = vec4(0.0);
    return;
  }
  gl_Position = u_projection * vec4(place, 1.0);
  gl_PointSize = u_size;
  // A point with its own colour, a star, is drawn in it; any other in its kind's.
  vec4 base = a_colour.a > 0.0 ? a_colour : u_colours[int(a_kind)];
  v_colour = base * vec4(1.0, 1.0, 1.0, u_opacity);
}`;

export const POINT_FRAGMENT = `#version 300 es
precision mediump float;
in vec4 v_colour;
out vec4 colour;
void main() {
  vec2 c = gl_PointCoord * 2.0 - 1.0;
  float r = dot(c, c);
  if (r > 1.0) discard;
  colour = vec4(v_colour.rgb, v_colour.a * (1.0 - r * r));
}`;

// Outlines as ribbons (ribbon.ts): each corner of a segment's two triangles placed at its end of
// the segment, then moved across the segment on the view by half the line's width and a pixel
// more, the pixel in which the fragment shader fades its edge so that a thin line neither breaks
// into dashes nor crawls as the view moves.
export const LINE_VERTEX = `#version 300 es
in vec3 a_from;
in vec3 a_to;
// Which end of its segment the corner is at, 0 or 1, and on which side of it, -1 or 1.
in vec2 a_corner;
uniform mat3 u_axes;
uniform vec3 u_centre;
uniform mat4 u_projection;
// Device pixels per clip-space unit across and up the view, and the line's width in them.
uniform vec2 u_pixels;
uniform float u_width;
// How much the offsets are scaled about the centre: 1 for an outline, and for a symbol drawn at
// a fixed size on the view the megaparsecs its unit spans at the centre's distance.
uniform float u_scale;
// For lines that fade where they crowd, the distance rings: the megaparsecs between neighbours
// (none, 0, for any other line), the ICRS point the offsets are from, and the device pixels
// between neighbours on the view at which a line is gone and at which it is whole.
uniform float u_fadeStep;
uniform vec3 u_origin;
uniform vec2 u_fadeRange;
// How far the fragment lies across the line from its middle, device pixels, and how faded.
out float v_across;
out float v_fade;
// Just in front of the eye: an end behind it is moved along its segment to here, so that the
// segment's direction on the view is defined.
const float NEAR = 1e-9;
vec4 inFront(vec4 end, vec4 other) {
  return end.w >= NEAR ? end : mix(end, other, (NEAR - end.w) / (other.w - end.w));
}
// How whole a ring is at the offset, at the place from the eye: by how far apart on the view it and its
// neighbours lie there, half a step either side of it out from the barycentre.
float fadeAt(vec3 offset, vec3 place) {
  if (u_fadeStep <= 0.0) return 1.0;
  vec3 outward = u_axes * normalize(offset + u_origin) * (0.5 * u_fadeStep);
  vec4 inner = u_projection * vec4(place - outward, 1.0);
  vec4 outer = u_projection * vec4(place + outward, 1.0);
  if (inner.w < NEAR || outer.w < NEAR) return 1.0;
  float apart = length((outer.xy / outer.w - inner.xy / inner.w) * u_pixels);
  return smoothstep(u_fadeRange.x, u_fadeRange.y, apart);
}
void main() {
  vec3 fromPlace = u_axes * (a_from * u_scale) + u_centre;
  vec3 toPlace = u_axes * (a_to * u_scale) + u_centre;
  vec4 a = u_projection * vec4(fromPlace, 1.0);
  vec4 b = u_projection * vec4(toPlace, 1.0);
  v_fade = a_corner.x < 0.5 ? fadeAt(a_from * u_scale, fromPlace) : fadeAt(a_to * u_scale, toPlace);
  if (a.w < NEAR && b.w < NEAR) {
    // Wholly behind the eye: outside the clip volume, not drawn.
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }
  vec4 from = inFront(a, b);
  vec4 to = inFront(b, a);
  vec2 along = (to.xy / to.w - from.xy / from.w) * u_pixels;
  vec2 dir = length(along) > 0.0 ? normalize(along) : vec2(1.0, 0.0);
  float halfWidth = 0.5 * u_width + 1.0;
  v_across = a_corner.y * halfWidth;
  vec2 across = vec2(-dir.y, dir.x) * v_across / u_pixels;
  vec4 at = a_corner.x < 0.5 ? from : to;
  gl_Position = vec4(at.xy + across * at.w, at.zw);
}`;

export const LINE_FRAGMENT = `#version 300 es
precision mediump float;
uniform vec4 u_colour;
uniform highp float u_width;
in float v_across;
in float v_fade;
out vec4 colour;
void main() {
  // The share of the pixel the line covers: whole within it, falling off across its edge's pixel,
  // and a line thinner than a pixel as faint as it is thin.
  float cover = clamp(0.5 * u_width + 0.5 - abs(v_across), 0.0, 1.0) * min(u_width, 1.0);
  colour = vec4(u_colour.rgb, u_colour.a * cover * v_fade);
}`;
