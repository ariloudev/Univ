// The renderer's shaders: points with a soft round edge, and lines; both placed relative to the
// eye, each batch by its centre's offset from the eye and its own small offsets.

import { KINDS } from "./kinds.ts";

/** The colours of the kinds of value and, after them, of structures' markers. */
const COLOURS = KINDS.length + 1;

export const POINT_VERTEX = `#version 300 es
in vec3 a_offset;
in float a_kind;
in vec4 a_colour;
uniform mat3 u_axes;
uniform vec3 u_centre;
uniform mat4 u_projection;
uniform float u_size;
uniform vec4 u_colours[${COLOURS}];
uniform float u_opacity;
out vec4 v_colour;
void main() {
  gl_Position = u_projection * vec4(u_axes * a_offset + u_centre, 1.0);
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

export const LINE_VERTEX = `#version 300 es
in vec3 a_offset;
uniform mat3 u_axes;
uniform vec3 u_centre;
uniform mat4 u_projection;
void main() {
  gl_Position = u_projection * vec4(u_axes * a_offset + u_centre, 1.0);
}`;

export const LINE_FRAGMENT = `#version 300 es
precision mediump float;
uniform vec4 u_colour;
out vec4 colour;
void main() {
  colour = u_colour;
}`;
