/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#version 110

#define SERVO_ES2

precision highp float;

uniform sampler2D sTexture0;
uniform sampler2D sTexture1;
uniform sampler2D sTexture2;
uniform vec4 uBlendParams;
uniform vec4 uAtlasParams;
uniform vec2 uDirection;
uniform vec4 uFilterParams;

varying vec2 vPosition;
varying vec4 vColor;
varying vec2 vColorTexCoord;
varying vec2 vMaskTexCoord;
varying vec4 vBorderPosition;
varying vec4 vBorderRadii;
varying vec2 vDestTextureSize;
varying vec2 vSourceTextureSize;
varying float vBlurRadius;
varying vec4 vTileParams;
varying vec4 vClipInRect;
varying vec4 vClipOutRect;

#define COLOR_TEXTURE_0 sTexture0
#define COLOR_TEXTURE_1 sTexture1
#define MASK_TEXTURE sTexture1
#define Y_TEXTURE sTexture0
#define U_TEXTURE sTexture1
#define V_TEXTURE sTexture2

vec4 Texture(sampler2D sampler, vec2 texCoord) {
    return texture2D(sampler, texCoord);
}

float GetAlphaFromMask(vec4 mask) {
    return mask.a;
}

void SetFragColor(vec4 color) {
    gl_FragColor = color;
}

