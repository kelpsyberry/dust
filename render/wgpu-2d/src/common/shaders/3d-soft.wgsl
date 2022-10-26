struct VertOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> VertOutput {
    var vert_positions: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2(-1.0, 1.0),
        vec2(1.0, 1.0),
        vec2(-1.0, -1.0),
        vec2(1.0, -1.0),
    );

    var vert_uvs: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
    );

    var output: VertOutput;
    output.pos = vec4<f32>((*(&vert_positions))[vertex_index], 0.0, 1.0);
    output.uv = (*(&vert_uvs))[vertex_index];
    return output;
}

struct ScanlineFlags {
    master_brightness_control: u32,
    color_effects_control: u32,
    blend_coeffs: u32,
    brightness_coeff: u32,
}

@group(0) @binding(0) var t_output_2d: texture_2d<u32>;
@group(0) @binding(1) var<uniform> scanline_flags: array<array<ScanlineFlags, 192>, 2>;
@group(1) @binding(0) var t_output_3d: texture_2d<u32>;

fn rgb6_to_rgba32f(value: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(value & 0x3Fu) * (1.0 / 63.0),
        f32((value >> 6u) & 0x3Fu) * (1.0 / 63.0),
        f32((value >> 12u) & 0x3Fu) * (1.0 / 63.0),
        1.0,
    );
}

fn blend(a: vec4<f32>, b: vec4<f32>, coeff_a: f32, coeff_b: f32) -> vec4<f32> {
    return vec4<f32>(min(a.rgb * coeff_a + b.rgb * coeff_b, vec3<f32>(1.0)), 1.0);
}

@fragment
fn fs_main(
    @location(0) uv: vec2<f32>,
) -> @location(0) vec4<f32> {
    let screen_index = u32(uv.y * 2.0);
    let scanline_index = u32(fract(uv.y * 2.0) * 192.0);
    let scanline_flags = scanline_flags[screen_index][scanline_index];
    let pixel = textureLoad(t_output_2d, vec2<i32>(uv * vec2<f32>(256.0, 384.0)), 0);

    let uv_3d = fract(uv * vec2<f32>(1.0, 2.0));
    let pixel_3d_raw =
        textureLoad(t_output_3d, vec2<i32>(uv_3d * vec2<f32>(textureDimensions(t_output_3d))), 0).r;
    let pixel_3d = rgb6_to_rgba32f(pixel_3d_raw);
    let pixel_3d_alpha = (pixel_3d_raw >> 18u) & 0x1Fu;

    var top_rgb = rgb6_to_rgba32f(pixel.r);
    var bot_rgb = rgb6_to_rgba32f(pixel.g);
    if (pixel.r & (1u << 23u)) != 0u {
        top_rgb = pixel_3d;
        if pixel_3d_alpha == 0u {
            top_rgb = bot_rgb;
        }
    }
    if (pixel.g & (1u << 23u)) != 0u {
        bot_rgb = pixel_3d;
        if pixel_3d_alpha == 0u {
            bot_rgb = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    }

    let color_effect = (scanline_flags.color_effects_control >> 6u) & 3u;
    let top_mask = pixel.r >> 26u;
    let bot_mask = pixel.g >> 26u;
    let target_1_mask = scanline_flags.color_effects_control & 0x3Fu;
    let target_2_mask = scanline_flags.color_effects_control >> 8u;
    let top_matches = (top_mask & target_1_mask) != 0u;
    let bot_matches = (bot_mask & target_2_mask) != 0u;
    let blend_coeff_a = f32(scanline_flags.blend_coeffs & 0x1Fu) * (1.0 / 16.0);
    let blend_coeff_b = f32(scanline_flags.blend_coeffs >> 16u) * (1.0 / 16.0);
    let brightness_coeff = f32(scanline_flags.brightness_coeff) * (1.0 / 16.0);

    var blended_rgb = top_rgb;
    if (pixel.r & (1u << 23u)) != 0u && bot_matches {
        blended_rgb = blend(top_rgb, bot_rgb, pixel_3d.a, 1.0 - pixel_3d.a);
    } else if (pixel.r & (1u << 24u)) != 0u && bot_matches {
        var coeff_a: f32;
        var coeff_b: f32;
        if (pixel.r & (1u << 25u)) != 0u {
            coeff_a = f32((pixel.r >> 18u) & 0xFu) * (1.0 / 16.0);
            coeff_b = 1.0 - coeff_a;
        } else {
            coeff_a = blend_coeff_a;
            coeff_b = blend_coeff_b;
        }
        blended_rgb = blend(top_rgb, bot_rgb, coeff_a, coeff_b);
    } else {
        switch color_effect {
            case 1u: {
                if top_matches && bot_matches {
                    blended_rgb = blend(top_rgb, bot_rgb, blend_coeff_a, blend_coeff_b);
                } else {
                    blended_rgb = top_rgb;
                }
            }

            case 2u: {
                blended_rgb = vec4(
                    top_rgb.rgb + (vec3<f32>(1.0) - top_rgb.rgb) * brightness_coeff,
                    1.0,
                );
            }

            case 3u: {
                blended_rgb = vec4(top_rgb.rgb - top_rgb.rgb * brightness_coeff, 1.0);
            }

            default: {
                blended_rgb = top_rgb;
            }
        }
    }

    let brightness_factor = f32(scanline_flags.master_brightness_control & 0x1Fu) * (1.0 / 16.0);
    let brightness_mode = scanline_flags.master_brightness_control >> 14u;
    switch brightness_mode {
        case 1u: {
            return vec4(
                blended_rgb.rgb + (vec3<f32>(1.0) - blended_rgb.rgb) * brightness_factor,
                1.0,
            );
        }

        case 2u: {
            return vec4(blended_rgb.rgb - blended_rgb.rgb * brightness_factor, 1.0);
        }

        default: {
            return blended_rgb;
        }
    }
}
