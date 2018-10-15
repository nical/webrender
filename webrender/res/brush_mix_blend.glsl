/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#define VECS_PER_SPECIFIC_BRUSH 3

#include shared,prim_shared,brush

varying vec3 vSrcUv;
varying vec3 vBackdropUv;
flat varying int vOp;

#ifdef WR_VERTEX_SHADER

//Note: this function is unsafe for `vi.world_pos.w <= 0.0`
vec2 snap_device_pos(VertexInfo vi, float device_pixel_scale) {
    return vi.world_pos.xy * device_pixel_scale / max(0.0, vi.world_pos.w) + vi.snap_offset;
}

void brush_vs(
    VertexInfo vi,
    int prim_address,
    RectWithSize local_rect,
    RectWithSize segment_rect,
    ivec3 user_data,
    mat4 transform,
    PictureTask pic_task,
    int brush_flags,
    vec4 unused
) {
    vec2 snapped_device_pos = snap_device_pos(vi, pic_task.common_data.device_pixel_scale);
    vec2 texture_size = vec2(textureSize(sPrevPassColor, 0));
    vOp = user_data.x;

    PictureTask src_task = fetch_picture_task(user_data.z);
    vec2 src_uv = snapped_device_pos +
                  src_task.common_data.task_rect.p0 -
                  src_task.content_origin;
    vSrcUv = vec3(src_uv / texture_size, src_task.common_data.texture_layer_index);

    RenderTaskCommonData backdrop_task = fetch_render_task_common_data(user_data.y);
    vec2 backdrop_uv = snapped_device_pos +
                       backdrop_task.task_rect.p0 -
                       src_task.content_origin;
    vBackdropUv = vec3(backdrop_uv / texture_size, backdrop_task.texture_layer_index);
}
#endif

#ifdef WR_FRAGMENT_SHADER
vec3 Multiply(vec3 Cb, vec3 Cs) {
    return Cb * Cs;
}

vec3 Screen(vec3 Cb, vec3 Cs) {
    return Cb + Cs - (Cb * Cs);
}

vec3 HardLight(vec3 Cb, vec3 Cs) {
    vec3 m = Multiply(Cb, 2.0 * Cs);
    vec3 s = Screen(Cb, 2.0 * Cs - 1.0);
    vec3 edge = vec3(0.5, 0.5, 0.5);
    return mix(m, s, step(edge, Cs));
}

#define if_then_else(cond, if_branch, else_branch) mix(else_branch, if_branch, cond)

float min_component(vec3 color) {
    return min(color.r, min(color.g, color.b));
}

float max_component(vec3 color) {
    return max(color.r, max(color.g, color.b));
}

vec3 color_dodge(vec3 cb, vec3 cs) {
    vec3 one = vec3(1.0);
    vec3 zero = vec3(0.0);
    vec3 color = min(one, cb / (one - cs));
    color = if_then_else(equal(cb, zero), zero, color);
    color = if_then_else(equal(cs, one), one, color);
    return color;
}

vec3 color_burn(vec3 cb, vec3 cs) {
    vec3 one = vec3(1.0);
    vec3 zero = vec3(0.0);
    vec3 color = one - min(one, (one - cb) / cs);
    color = if_then_else(equal(cb, one), one, color);
    color = if_then_else(equal(cs, zero), zero, color);
    return color;
}

vec3 soft_light(vec3 cb, vec3 cs) {
    vec3 one = vec3(1.0);
    vec3 color_0 = cb - (one - 2.0 * cs) * cb * (one - cb);
    vec3 sqrt_cb = vec3(sqrt(cb.r), sqrt(cb.g), sqrt(cb.b));
    vec3 d = ((16.0 * cb - vec3(12.0)) * cb + vec3(4.0)) * cb;
    d = if_then_else(lessThanEqual(cb, vec3(0.25)), d, sqrt_cb);
    vec3 color_1 = cb + (2.0 * cs - one) * (d - cb);
    return if_then_else(lessThanEqual(cs, vec3(0.5)), color_0, color_1);
}

vec3 Difference(vec3 Cb, vec3 Cs) {
    return abs(Cb - Cs);
}

vec3 Exclusion(vec3 Cb, vec3 Cs) {
    return Cb + Cs - 2.0 * Cb * Cs;
}


float Sat(vec3 color) {
    return max_component(color) - min_component(color);
}

float Lum(vec3 c) {
    vec3 f = vec3(0.3, 0.59, 0.11);
    return dot(c, f);
}

vec3 clip_color(vec3 color) {
    float l = Lum(color);
    float cmin = min_component(color);
    float cmax = max_component(color);

    let color_0 = l + (((color - l) * l) / (l - cmin));
    let color_1 = l + (((color - l) * (1.0 - l)) / (cmax - l));

    color = if_then_else(lessThan(cmin, 0.0), color_0, color);
    color = if_then_else(greaterThan(cmax, 1.0), color_1, color);

    return color;
}

vec3 SetLum(vec3 color, float l) {
    float d = l - Lum(color);
    return clip_color(color + d);
}

void SetSatInner(inout float Cmin, inout float Cmid, inout float Cmax, float s) {
    if (Cmax > Cmin) {
        Cmid = (((Cmid - Cmin) * s) / (Cmax - Cmin));
        Cmax = s;
    } else {
        Cmid = 0.0;
        Cmax = 0.0;
    }
    Cmin = 0.0;
}

vec3 SetSat(vec3 C, float s) {
    if (C.r <= C.g) {
        if (C.g <= C.b) {
            SetSatInner(C.r, C.g, C.b, s);
        } else {
            if (C.r <= C.b) {
                SetSatInner(C.r, C.b, C.g, s);
            } else {
                SetSatInner(C.b, C.r, C.g, s);
            }
        }
    } else {
        if (C.r <= C.b) {
            SetSatInner(C.g, C.r, C.b, s);
        } else {
            if (C.g <= C.b) {
                SetSatInner(C.g, C.b, C.r, s);
            } else {
                SetSatInner(C.b, C.g, C.r, s);
            }
        }
    }
    return C;
}

vec3 Hue(vec3 Cb, vec3 Cs) {
    return SetLum(SetSat(Cs, Sat(Cb)), Lum(Cb));
}

vec3 Saturation(vec3 Cb, vec3 Cs) {
    return SetLum(SetSat(Cb, Sat(Cs)), Lum(Cb));
}

vec3 Color(vec3 Cb, vec3 Cs) {
    return SetLum(Cs, Lum(Cb));
}

vec3 Luminosity(vec3 Cb, vec3 Cs) {
    return SetLum(Cb, Lum(Cs));
}

const int MixBlendMode_Multiply    = 1;
const int MixBlendMode_Screen      = 2;
const int MixBlendMode_Overlay     = 3;
const int MixBlendMode_Darken      = 4;
const int MixBlendMode_Lighten     = 5;
const int MixBlendMode_ColorDodge  = 6;
const int MixBlendMode_ColorBurn   = 7;
const int MixBlendMode_HardLight   = 8;
const int MixBlendMode_SoftLight   = 9;
const int MixBlendMode_Difference  = 10;
const int MixBlendMode_Exclusion   = 11;
const int MixBlendMode_Hue         = 12;
const int MixBlendMode_Saturation  = 13;
const int MixBlendMode_Color       = 14;
const int MixBlendMode_Luminosity  = 15;

Fragment brush_fs() {
    vec4 Cb = textureLod(sPrevPassColor, vBackdropUv, 0.0);
    vec4 Cs = textureLod(sPrevPassColor, vSrcUv, 0.0);

    if (Cb.a == 0.0) {
        return Fragment(Cs);
    }
    if (Cs.a == 0.0) {
        return Fragment(vec4(0.0));
    }

    // The mix-blend-mode functions assume no premultiplied alpha
    Cb.rgb /= Cb.a;
    Cs.rgb /= Cs.a;

    // Return yellow if none of the branches match (shouldn't happen).
    vec4 result = vec4(1.0, 1.0, 0.0, 1.0);

    switch (vOp) {
        case MixBlendMode_Multiply:
            result.rgb = Multiply(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Screen:
            result.rgb = Screen(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Overlay:
            // Overlay is inverse of Hardlight
            result.rgb = HardLight(Cs.rgb, Cb.rgb);
            break;
        case MixBlendMode_Darken:
            result.rgb = min(Cs.rgb, Cb.rgb);
            break;
        case MixBlendMode_Lighten:
            result.rgb = max(Cs.rgb, Cb.rgb);
            break;
        case MixBlendMode_ColorDodge:
            result.rgb = color_dodge(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_ColorBurn:
            result.rgb = color_burn(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_HardLight:
            result.rgb = HardLight(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_SoftLight:
            result.rgb = soft_light(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Difference:
            result.rgb = Difference(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Exclusion:
            result.rgb = Exclusion(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Hue:
            result.rgb = Hue(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Saturation:
            result.rgb = Saturation(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Color:
            result.rgb = Color(Cb.rgb, Cs.rgb);
            break;
        case MixBlendMode_Luminosity:
            result.rgb = Luminosity(Cb.rgb, Cs.rgb);
            break;
        default: break;
    }

    result.rgb = (1.0 - Cb.a) * Cs.rgb + Cb.a * result.rgb;
    result.a = Cs.a;

    result.rgb *= result.a;

    return Fragment(result);
}
#endif
