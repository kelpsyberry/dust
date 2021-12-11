#version 450

layout(set = 1, binding = 0) uniform sampler u_Sampler;
layout(set = 1, binding = 1) uniform texture2D u_Texture;

layout(location = 0) in vec4 v_Color;
layout(location = 1) in vec2 v_UV;

layout(location = 0) out vec4 o_Target;

vec4 srgb_to_linear(vec4 srgb) {
  vec3 gamma_corrected_scale = ceil(srgb.rgb - vec3(0.04045));
  vec3 linear_scaled = srgb.rgb / vec3(12.92);
  vec3 gamma_corrected = pow((srgb.rgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
  return vec4(mix(linear_scaled, gamma_corrected, gamma_corrected_scale), srgb.a);
}

void main() {
  #ifdef SRGB
    o_Target = srgb_to_linear(v_Color) * texture(sampler2D(u_Texture, u_Sampler), v_UV);
  #else
    o_Target = v_Color * texture(sampler2D(u_Texture, u_Sampler), v_UV);
  #endif
}
