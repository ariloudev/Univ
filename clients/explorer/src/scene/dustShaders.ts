// The dust map's shaders (draw-the-dust-map): the dust column along a segment from the eye, which
// the point shader dims each star by and the haze pass draws the dust with, and the haze pass.
// Both clip the segment to the grid and sample it at even steps, about one a cell crossed, as
// dustGrid.ts's `columnTo`.

/** The most steps of the haze's integral along each pixel's ray. */
const HAZE_STEPS = 96;

/** How strongly the dust column turns the haze opaque, for display. */
const HAZE = "1.2";
/** The haze's colour: the brown of dust lit by starlight. */
const HAZE_COLOUR = "vec3(0.62, 0.45, 0.32)";

/** The uniforms and function both shaders share. `u_dustEye` and `u_dustAxes` are dustGrid.ts's
 * `DustFrame`; the texture holds densities in thousandths of the grid's unit. */
export const DUST_COLUMN = `
uniform highp sampler3D u_dust;
uniform float u_dustOn;
uniform vec3 u_dustEye;
uniform mat3 u_dustAxes;
// The dust column from the eye to \`place\` (the eye's axes, megaparsecs) in the grid's unit times
// parsecs, over about one sample a cell of the part of the segment inside the grid, \`most\` at most.
float dustColumn(vec3 place, int most) {
  if (u_dustOn < 0.5) return 0.0;
  vec3 d = u_dustAxes * place;
  vec3 safe = mix(d, vec3(1e-9), lessThan(abs(d), vec3(1e-9)));
  vec3 a = -u_dustEye / safe;
  vec3 b = (vec3(1.0) - u_dustEye) / safe;
  vec3 lo = min(a, b);
  vec3 hi = max(a, b);
  float t0 = max(max(max(lo.x, lo.y), lo.z), 0.0);
  float t1 = min(min(min(hi.x, hi.y), hi.z), 1.0);
  if (t1 <= t0) return 0.0;
  int steps = clamp(int(ceil((t1 - t0) * length(d) * float(textureSize(u_dust, 0).x))), 1, most);
  float sum = 0.0;
  for (int s = 0; s < ${HAZE_STEPS}; s++) {
    if (s >= steps) break;
    sum += texture(u_dust, u_dustEye + d * mix(t0, t1, (float(s) + 0.5) / float(steps))).r;
  }
  return sum * 1.0e-3 * (t1 - t0) * length(place) * 1.0e6 / float(steps);
}`;

export const HAZE_VERTEX = `#version 300 es
out vec2 v_ndc;
void main() {
  // One triangle covering the view.
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
  v_ndc = p;
  gl_Position = vec4(p, 0.0, 1.0);
}`;

export const HAZE_FRAGMENT = `#version 300 es
precision highp float;
precision highp sampler3D;
in vec2 v_ndc;
uniform mat4 u_projection;
// How far along each ray, megaparsecs, the grid is left behind.
uniform float u_reach;
out vec4 colour;
${DUST_COLUMN}
void main() {
  vec3 ray = normalize(vec3(v_ndc.x / u_projection[0][0], v_ndc.y / u_projection[1][1], -1.0));
  float column = dustColumn(ray * u_reach, ${HAZE_STEPS});
  colour = vec4(${HAZE_COLOUR}, 1.0 - exp(-${HAZE} * column));
}`;
