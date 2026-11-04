// The renderer's shaders: points with a soft round edge, and lines; both placed relative to the
// eye, each batch by its centre's offset from the eye and its own small offsets.

export const POINT_VERTEX = `#version 300 es
in vec3 a_offset;
in float a_kind;
uniform mat3 u_axes;
uniform vec3 u_centre;
uniform mat4 u_projection;
uniform float u_size;
uniform vec4 u_colours[5];
out vec4 v_colour;
void main() {
  gl_Position = u_projection * vec4(u_axes * a_offset + u_centre, 1.0);
  gl_PointSize = u_size;
  v_colour = u_colours[int(a_kind)];
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
