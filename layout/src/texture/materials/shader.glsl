#version 150
// The material shader: one surface lit per device pixel. The CPU's twin is
// `shade.rs` + `surface.rs`; both follow the AzDrive design's renderer step by
// step, so the two backends draw the same surface. The first line is replaced
// with the `#version` the driver accepted. The frame is rendered into a
// texture whose first row is the surface's TOP (WebRender shows texel row 0
// at the top), so `p.y` grows with `gl_FragCoord.y`.
precision highp float;
precision highp int;
uniform vec2 uSize;        // the frame, device px
uniform float uDpr;
uniform vec2 uOriginPx;    // the surface's top-left in the window, device px (grain and dither lie on the window's pixels)
uniform vec4 uRadii;       // logical px: top-left, top-right, bottom-right, bottom-left
uniform int uShape;        // 0 rounded rect, 1 the distance field in uShapeTex
uniform sampler2D uShapeTex;
uniform float uShapeRange; // logical px the distance field's 0..1 spans (signed, 0.5 on the edge)
uniform vec4 uBevel;       // width (logical px), depth, softness 0..1, inset (1) or raised (0)
uniform int uMat;          // 0 leather, 1 aluminium, 2 denim, 3 vellum
uniform int uSource;       // 0 the tile, 1 procedural, 2 smooth (no grain)
uniform sampler2D uTile;   // R luma, G/B the tile's normal (leather), A occlusion
uniform float uTileNormal;
uniform sampler2D uMacro;  // the tile's large-scale mottling
uniform float uMacroPeriod;
uniform vec3 uTint;        // linear
uniform vec2 uRough;       // the roughness range the surface is spread over
uniform float uRelief;
uniform float uVariation;  // how far the luma moves the tint
uniform float uMetal;
uniform float uAniso;
uniform float uScale;      // the pattern's size
uniform vec3 uL;           // toward the sun
uniform vec3 uLightColor;
uniform float uAmbient;
uniform sampler2D uEnv;    // the wallpaper behind the window
uniform vec4 uEnvRect;     // this surface on its screen: x, y (logical px), the screen's width, height
uniform vec3 uEnvFlat;     // the highlight colour when there is no wallpaper (linear)
uniform float uEnvMode;    // 1 the wallpaper, 0 the flat highlight colour
uniform float uGrain;      // the leather's grain cell, logical px
uniform vec2 uCushion;     // the padding: how far in from the edge it rises (px) and how high
uniform vec2 uPillow;      // the pillow: as far in (px) and as high
uniform float uPillowFoot; // how much of the pillow's roll the bottom edge keeps
uniform vec4 uGlow;        // the accent's glow: rgb (linear, times strength), its reach (px)
uniform int uLightMode;    // 0 the sun (uL), 1 a point light at uLightPos
uniform vec3 uLightPos;    // the point light over the desktop: x, y (desktop points), height
uniform vec2 uDesktopOrigin; // the surface's top-left on the desktop
out vec4 outColor;

const float PI = 3.14159265;
float hash12(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
vec2 hash22(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973)); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.xx + p3.yz) * p3.zy); }
float vnoise(vec2 p) { vec2 i = floor(p), f = fract(p); vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash12(i), hash12(i + vec2(1, 0)), u.x), mix(hash12(i + vec2(0, 1)), hash12(i + vec2(1, 1)), u.x), u.y); }
float fbm(vec2 p) { float s = 0.0, a = 0.5; for (int i = 0; i < 4; i++) { s += a * vnoise(p); p = p * 2.03 + 17.1; a *= 0.5; } return s / 0.9375; }
vec2 worley(vec2 p) { vec2 i = floor(p), f = fract(p); float d1 = 8.0, d2 = 8.0;
  for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) { vec2 g = vec2(x, y); vec2 o = hash22(i + g) * 0.8 + 0.1;
    float d = length(g + o - f); if (d < d1) { d2 = d1; d1 = d; } else if (d < d2) { d2 = d; } }
  return vec2(d1, d2); }
vec2 grainCells(vec2 p, float gs, float seed) {
  vec2 q = p / gs + 0.45 * vec2(vnoise(p * 0.6 / gs + seed), vnoise(p * 0.6 / gs + 19.7 + seed)) + seed;
  vec2 w = worley(q);
  float edge = w.y - w.x;
  return vec2(sqrt(smoothstep(0.0, 0.6, edge)), 1.0 - smoothstep(0.0, 0.14, edge));
}
vec4 leatherP(vec2 p) {
  float gs = uGrain;
  float m = smoothstep(0.3, 0.7, fbm(p * 0.006 + 4.2));
  vec2 g1 = grainCells(p, gs * 0.85, 0.0), g2 = grainCells(p, gs * 1.25, 37.0);
  float dome = mix(g1.x, g2.x, m), crease = mix(g1.y, g2.y, m);
  float lod = clamp((gs * uDpr * uScale - 1.1) / 1.6, 0.0, 1.0);
  float pores = fbm(p * 2.2 / gs);
  vec2 wp = p * 0.02 + 1.6 * vec2(vnoise(p * 0.006 + 2.0), vnoise(p * 0.006 + 8.0));
  float wrinkle = 1.0 - abs(fbm(wp + 3.1) * 2.0 - 1.0);
  wrinkle = 0.55 * wrinkle * wrinkle;
  float fold = fbm(p * 0.007 + 9.0);
  float h = lod * gs * (0.05 * dome - 0.035 * crease + 0.012 * pores) + 0.55 * wrinkle + 1.3 * fold;
  float mottle = fbm(p * 0.0045 + 1.7) - 0.5, blotch = fbm(p * 0.017 + 8.3) - 0.5;
  float luma = 0.62 + lod * (0.035 * dome - 0.09 * crease + 0.02 * (pores - 0.5)) + 0.05 * (wrinkle - 0.5) + 0.06 * (fold - 0.5)
             + 0.16 * mottle + 0.07 * blotch;
  return vec4(h, luma, clamp(0.35 + 0.45 * crease * lod + 0.2 * (1.0 - dome), 0.0, 1.0), 1.0 - 0.18 * crease * lod);
}
vec4 aluminiumP(vec2 p) {
  float a = vnoise(vec2(p.x * 0.012, p.y * 1.7)), b = vnoise(vec2(p.x * 0.05, p.y * 4.3) + 7.0), c = vnoise(vec2(p.x * 0.4, p.y * 11.0) + 3.0);
  float d = vnoise(vec2(p.x * 0.35, p.y * 3.0) + 11.0), f = vnoise(vec2(p.x * 1.3, p.y * 7.0) + 23.0);
  float lines = 0.38 * a + 0.22 * b + 0.12 * c + 0.16 * d + 0.12 * f;
  return vec4(0.26 * lines, 0.72 + 0.36 * lines + 0.08 * (fbm(p * 0.03) - 0.5), 0.3 + 0.7 * (1.0 - lines), 1.0);
}
vec4 denimP(vec2 p) {
  vec2 c = floor(p / 2.1), f = fract(p / 2.1);
  float over = step(mod(c.x - c.y + 400.0, 4.0), 2.5);
  float warp = sin(f.x * PI), weft = sin(f.y * PI);
  float slub = vnoise(vec2(p.x * 0.6, p.y * 0.035));
  float fib = fbm(vec2(p.x * 1.4, p.y * 4.0) * mix(0.6, 1.0, over) + over * 23.0);
  float h = over > 0.5 ? 0.55 * warp + 0.15 * fib : 0.35 * weft + 0.15 * fib;
  float luma = over > 0.5 ? 0.32 + 0.28 * slub + 0.15 * fib : 0.86 + 0.1 * fib;
  return vec4(h, luma, 0.6 + 0.4 * fib, 0.72 + 0.28 * (over > 0.5 ? warp : weft));
}
vec4 vellumP(vec2 p) {
  float blot = fbm(p * 0.012);
  float fib = fbm(vec2(p.x * 0.09 + p.y * 0.03, p.y * 0.35));
  float cr = pow(1.0 - abs(fbm(p * 0.04) * 2.0 - 1.0), 6.0);
  float speck = step(0.992, hash12(floor(p * 1.5)));
  return vec4(0.6 * blot + 0.12 * fib - 0.3 * cr, 0.94 - 0.16 * blot - 0.08 * fib - 0.22 * cr - 0.3 * speck, 0.85, 1.0 - 0.3 * cr);
}
// The tile, upsampled with a cubic B-spline (four bilinear taps) so its
// texels never show as squares, plus micro-detail finer than a texel.
vec4 cubicW(float v) { vec4 n = vec4(1.0, 2.0, 3.0, 4.0) - v; vec4 s = n * n * n;
  float x = s.x, y = s.y - 4.0 * s.x, z = s.z - 4.0 * s.y + 6.0 * s.x, w = 6.0 - x - y - z; return vec4(x, y, z, w) * (1.0 / 6.0); }
vec4 bicubic(sampler2D t, vec2 uv) {
  vec2 size = vec2(textureSize(t, 0)), inv = 1.0 / size; uv = uv * size - 0.5;
  vec2 fxy = fract(uv); uv -= fxy; vec4 xc = cubicW(fxy.x), yc = cubicW(fxy.y);
  vec4 c = uv.xxyy + vec2(-0.5, 1.5).xyxy; vec4 s = vec4(xc.xz + xc.yw, yc.xz + yc.yw);
  vec4 o = (c + vec4(xc.yw, yc.yw) / s) * inv.xxyy;
  vec4 s0 = texture(t, o.xz), s1 = texture(t, o.yz), s2 = texture(t, o.xw), s3 = texture(t, o.yw);
  float sx = s.x / (s.x + s.y), sy = s.z / (s.z + s.w);
  return mix(mix(s3, s2, sx), mix(s1, s0, sx), sy);
}
vec4 tileS(vec2 p) {
  vec4 t = bicubic(uTile, p / 128.0);
  float m = uMacroPeriod > 0.0 ? 0.82 + 0.36 * texture(uMacro, p / uMacroPeriod).r : 1.0;
  float detail = vnoise(p * 1.7) - 0.5 + 0.5 * (vnoise(p * 3.9 + 5.0) - 0.5);
  float h = 1.1 * t.r + 0.16 * detail;
  return vec4(h, clamp(t.r * m + 0.06 * detail, 0.0, 1.0), 1.0 - t.r, t.a);
}
vec4 surf(vec2 p) {
  p /= uScale;
  if (uSource == 2) return vec4(0.0, 0.62, 0.5, 1.0);
  if (uSource == 0) return tileS(p);
  if (uMat == 0) return leatherP(p);
  if (uMat == 1) return aluminiumP(p);
  if (uMat == 2) return denimP(p);
  return vellumP(p);
}

// The shape: its signed distance (negative inside).
float sdRound(vec2 p, vec2 half_, vec4 r) {
  float rr = p.x < 0.0 ? (p.y < 0.0 ? r.x : r.w) : (p.y < 0.0 ? r.y : r.z);
  rr = min(rr, min(half_.x, half_.y));
  vec2 q = abs(p) - half_ + rr; return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - rr;
}
float shapeD(vec2 p) {
  vec2 sizeL = uSize / uDpr;
  if (uShape == 0) return sdRound(p - sizeL * 0.5, sizeL * 0.5, uRadii);
  return (texture(uShapeTex, p / sizeL).r - 0.5) * 2.0 * uShapeRange;
}

// The pillow follows the surface's outline: its contours run parallel to
// every edge and round at every corner, because each edge's roll blends into
// the next over a wide band (a polynomial smooth minimum); the cross-section
// is a raised cosine, level at the seam and at the crown.
float smin(float a, float b, float k) { float h = max(k - abs(a - b), 0.0) / k; return min(a, b) - h * h * k * 0.25; }
float pillowDist(vec2 p, float d) {
  vec2 sizeL = uSize / uDpr;
  if (uShape != 0) return max(-d, 0.0);
  float bottom = uPillowFoot > 0.0 ? (sizeL.y - p.y) / uPillowFoot : 1e5;
  float k = uPillow.x * 0.75;
  float across = smin(p.y, bottom, k);
  float along = smin(p.x, sizeL.x - p.x, k);
  return max(min(smin(across, along, k), -d), 0.0);
}
float pillowT(vec2 p) { return uPillow.x > 0.0 ? 0.5 - 0.5 * cos(3.14159265 * clamp(pillowDist(p, shapeD(p)) / uPillow.x, 0.0, 1.0)) : 1.0; }
float pillowH(vec2 p) { return uPillow.x > 0.0 ? uPillow.y * pillowT(p) : 0.0; }

// A bevel along the shape: a chisel when hard, a cushion when soft (no slope
// at the edge or the crown, so it draws no line); plus the padding.
float bevelH(float d) {
  float t = clamp(-d / max(uBevel.x, 1e-3), 0.0, 1.0);
  float prof = mix(t, t * t * (3.0 - 2.0 * t), uBevel.z);
  float b = uBevel.y * uBevel.x * 0.5 * (uBevel.w > 0.5 ? 1.0 - prof : prof);
  float c = uCushion.x > 0.0 ? uCushion.y * smoothstep(0.0, 1.0, clamp(-d / uCushion.x, 0.0, 1.0)) : 0.0;
  return b + c;
}

float ggx(float nh, float a) { float a2 = a * a; float d = nh * nh * (a2 - 1.0) + 1.0; return a2 / (PI * d * d); }
float ggxAniso(vec3 n, vec3 h, float ax, float ay) {
  vec3 t = normalize(vec3(1.0, 0.0, 0.0) - n * n.x); vec3 b = cross(n, t);
  float th = dot(t, h) / ax, bh = dot(b, h) / ay, nh = dot(n, h); float d = th * th + bh * bh + nh * nh;
  return 1.0 / (PI * ax * ay * d * d);
}
float smithG(float nl, float nv, float a) { float k = a * 0.5; return (nl / (nl * (1.0 - k) + k)) * (nv / (nv * (1.0 - k) + k)); }
vec3 schlick(vec3 f0, float c) { return f0 + (1.0 - f0) * pow(1.0 - c, 5.0); }
vec3 toLinear(vec3 c) { return pow(c, vec3(2.2)); }
vec3 encode(vec3 c) { c = clamp(c, 0.0, 1.0); return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c)); }

// What a metal reflects: the wallpaper behind this point of the window,
// moved by the tilt of the surface and blurred by its roughness.
vec3 envAt(vec2 p, vec3 n, float rough) {
  if (uEnvMode < 0.5) return uEnvFlat;
  vec2 screen = uEnvRect.xy + p + n.xy * 220.0;
  vec2 uv = clamp(screen / uEnvRect.zw, 0.0, 1.0);
  return toLinear(textureLod(uEnv, uv, 1.5 + rough * 7.0).rgb);
}

void main() {
  vec2 p = gl_FragCoord.xy / uDpr;
  float d = shapeD(p);
  float alpha = clamp(0.5 - d * uDpr, 0.0, 1.0);
  if (alpha <= 0.0) { outColor = vec4(0.0); return; }
  vec2 g = (gl_FragCoord.xy + uOriginPx) / uDpr;
  float e = 0.5 / uDpr;
  vec4 s = surf(g);
  vec4 sx0 = surf(g - vec2(e, 0.0)), sx1 = surf(g + vec2(e, 0.0)), sy0 = surf(g - vec2(0.0, e)), sy1 = surf(g + vec2(0.0, e));
  vec2 grad = vec2(sx1.x - sx0.x, sy1.x - sy0.x) / (2.0 * e);
  vec3 n = normalize(vec3(-grad * uRelief, 1.0));
  if (uSource == 0 && uTileNormal > 0.5) {
    vec4 t = bicubic(uTile, (g / uScale) / 128.0);
    vec2 tn = vec2(t.g * 2.0 - 1.0, -(t.b * 2.0 - 1.0)) * 1.4 * uRelief;
    n = normalize(vec3(tn - grad * 0.25 * uRelief, 1.0));
  }
  // the shape's slope - bevel, padding, pillow - from its own gradient
  if ((uBevel.x > 0.0 && uBevel.y > 0.0) || uCushion.x > 0.0 || uPillow.x > 0.0) {
    float bx = (bevelH(shapeD(p + vec2(e, 0.0))) + pillowH(p + vec2(e, 0.0)) - bevelH(shapeD(p - vec2(e, 0.0))) - pillowH(p - vec2(e, 0.0))) / (2.0 * e);
    float by = (bevelH(shapeD(p + vec2(0.0, e))) + pillowH(p + vec2(0.0, e)) - bevelH(shapeD(p - vec2(0.0, e))) - pillowH(p - vec2(0.0, e))) / (2.0 * e);
    vec2 bg = vec2(bx, by);
    float bl = length(bg);
    if (bl > 3.7) bg *= 3.7 / bl;   // no slope past 75 degrees: the seam, not a cliff
    vec3 bn = normalize(vec3(-bg, 1.0));
    n = normalize(vec3(bn.xy * n.z + n.xy, n.z * bn.z));
  }

  if (uPillow.x > 0.0) s.w *= mix(0.9, 1.0, pillowT(p));
  vec3 V = vec3(0.0, 0.0, 1.0);
  float nv = max(n.z, 1e-3);
  vec3 albedo = uTint * clamp(1.0 + uVariation * (s.y - 0.62) * 2.0, 0.0, 2.0);
  float rough = mix(uRough.x, uRough.y, s.z);
  float a = max(rough * rough, 0.002);
  vec3 f0 = mix(vec3(0.04), albedo, uMetal);
  vec3 hemi = vec3(clamp(1.0 - 0.35 * n.y, 0.55, 1.45));

  vec3 L = uLightMode == 1 ? normalize(uLightPos - vec3(uDesktopOrigin + p, 0.0)) : normalize(uL);
  vec3 H = normalize(L + V);
  float nl = max(dot(n, L), 0.0), nh = max(dot(n, H), 0.0), vh = max(dot(V, H), 0.0);
  float D = uAniso > 0.5 ? ggxAniso(n, H, max(a * 0.35, 0.002), max(a * 1.8, 0.01)) : ggx(nh, a);
  vec3 spec = D * smithG(max(nl, 1e-3), nv, a) * schlick(f0, vh) / max(4.0 * nl * nv, 1e-4) * nl;
  float wrap = 0.45 * (1.0 - uMetal);
  float nlw = max((dot(n, L) + wrap) / (1.0 + wrap), 0.0), flatw = (L.z + wrap) / (1.0 + wrap);
  vec3 diffuse = albedo * (1.0 - uMetal) * (uAmbient * s.w * hemi + (1.0 - uAmbient) * nlw / max(flatw, 0.05));
  vec3 R = reflect(-V, n);
  float up = -R.y;
  vec3 studio = vec3(0.28 + 0.34 * smoothstep(-0.5, 0.0, up) + 0.72 * smoothstep(0.0, 0.35, up) + 0.5 * pow(max(dot(R, L), 0.0), 12.0));
  vec3 room = mix(studio, envAt(p, n, rough) * 1.25, 0.45);
  vec3 env = schlick(f0, nv) * mix(studio * 1.1, room, uMetal) * mix(0.25, 1.0, 1.0 - rough) * s.w;
  vec3 satin = uMetal * albedo * (0.22 * hemi + 0.3 * nl);
  vec3 col = (diffuse + satin) * uLightColor + spec * uLightColor * (1.0 - uAmbient) + env * mix(0.55 * (1.0 - 0.5 * rough), 1.0, uMetal);
  if (uGlow.w > 0.0) {
    float below = (uSize.y / uDpr) - p.y;
    col += uGlow.rgb * exp(-below / uGlow.w) * (0.55 + 0.45 * s.w) * mix(1.0, 0.5, uMetal);
  }
  // a soft shoulder above 0.86: highlights roll toward white as on film
  float peak = max(col.r, max(col.g, col.b));
  if (peak > 0.86) {
    float k = (0.86 + 0.14 * (1.0 - exp(-(peak - 0.86) / 0.14))) / peak;
    col = mix(col * k, vec3(dot(col * k, vec3(0.2126, 0.7152, 0.0722))), clamp((peak - 0.86) * 0.4, 0.0, 0.25));
  }
  // dithered on the window's pixels, so no gradient bands and no seam
  vec3 outc = encode(col) + (hash12(gl_FragCoord.xy + uOriginPx + 0.37) - 0.5) / 255.0;
  outColor = vec4(outc * alpha, alpha);
}
