(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const HORIZON: f32 = 0.06;
       const EYE: f32 = 1.25;
       const FIRE: f32 = 4.2;

       fn heat(t: f32) -> vec3f {
           let x = clamp(t, 0.0, 1.5);
           return vec3f(1.0, 0.30, 0.05) * x * 1.6
               + vec3f(0.0, 0.42, 0.10) * x * x * 1.4
               + vec3f(0.0, 0.10, 0.35) * pow(x, 4.0) * 1.2;
       }

       fn flicker() -> f32 {
           return 0.82 + 0.12 * noise2(vec2f(frame.time * 7.0, 1.0)) + 0.08 * noise2(vec2f(frame.time * 17.0, 3.0));
       }

       fn tongue(p: vec2f, seed: f32, tall: f32) -> f32 {
           let q = vec2f(p.x, p.y / tall);
           if (q.y < -0.05 || q.y > 1.3) {
               return 0.0;
           }
           let rising = vec2f(q.x * 4.0 + seed * 7.0, q.y * 2.4 - frame.time * 2.9);
           let n = fbm2(rising, 5);
           let sway = (0.06 * sin(frame.time * 1.9 + seed * 5.0) + (n - 0.5) * 0.45) * q.y;
           let x = q.x - sway;
           let width = 0.26 * pow(max(1.0 - q.y / 1.15, 0.0), 0.8) * (0.7 + 0.3 * smoothstep(-0.1, 0.15, q.y));
           let body = 1.0 - abs(x) / max(width, 0.0001);
           return clamp(body * 1.4 - (n - 0.42) * 2.2 * q.y - q.y * 0.55, 0.0, 1.3);
       }

       fn flames(p: vec2f) -> f32 {
           var sum = tongue(p - vec2f(0.0, 0.0), 1.0, 1.15);
           sum = max(sum, tongue(p - vec2f(-0.15, -0.02), 2.0, 0.8) * 0.95);
           sum = max(sum, tongue(p - vec2f(0.14, -0.01), 3.0, 0.9) * 0.95);
           sum = max(sum, tongue(p - vec2f(-0.05, 0.02), 4.0, 0.6) * 1.1);
           return sum;
       }

       fn segment(p: vec2f, a: vec2f, b: vec2f) -> vec2f {
           let pa = p - a;
           let ba = b - a;
           let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
           return vec2f(length(pa - ba * h), h);
       }

       fn logs(p: vec2f, light: f32) -> vec4f {
           var color = vec3f(0.0);
           var cover = 0.0;
           let ends = array<vec4f, 5>(
               vec4f(-0.40, -0.13, -0.04, 0.20),
               vec4f(0.38, -0.14, 0.03, 0.21),
               vec4f(-0.20, -0.17, 0.02, 0.24),
               vec4f(0.22, -0.18, -0.03, 0.23),
               vec4f(-0.46, -0.15, 0.44, -0.12));
           let radii = array<f32, 5>(0.042, 0.045, 0.038, 0.040, 0.05);
           for (var i = 0; i < 5; i++) {
               let e = ends[i];
               let s = segment(p, e.xy, e.zw);
               let r = radii[i];
               if (s.x < r) {
                   let across = s.x / r;
                   let bark = fbm2(vec2f(s.y * 30.0 + f32(i) * 5.0, across * 3.0), 4);
                   let top = sqrt(max(1.0 - across * across, 0.0));
                   let base = vec3f(0.030, 0.016, 0.008) * (0.3 + 0.9 * bark);
                   let near = exp(-length(p - vec2f(0.0, 0.08)) * 4.0);
                   let facing = smoothstep(-0.3, 1.0, across * sign(e.z - e.x) * sign(p.x + 0.0001));
                   let lit = base * (0.10 + light * near * 1.4 * (0.25 + 0.75 * top) * (0.6 + 0.4 * facing));
                   let cracks = smoothstep(0.66, 0.80, fbm2(vec2f(s.y * 26.0, across * 4.0 + frame.time * 0.2), 5));
                   let embers = cracks * pow(near, 1.5) * (0.3 + 0.7 * s.y) * (1.0 - top * 0.6);
                   color = lit + heat(0.45 + 0.35 * noise2(vec2f(s.y * 20.0, frame.time * 2.0))) * embers * 0.5;
                   cover = smoothstep(r, r - 1.5 / frame.size.y, s.x);
               }
           }
           return vec4f(color, cover);
       }

       fn ground(uv: vec2f, light: f32) -> vec3f {
           let depth = EYE / max(HORIZON - uv.y, 0.0001);
           let world = vec2f(uv.x * depth, depth);
           let fire = vec2f(0.0, FIRE);
           let away = length(world - fire);
           let dirt = fbm2(world * 1.8, 5);
           let needles = smoothstep(0.55, 0.8, fbm2(world * 9.0, 3));
           let pebbles = smoothstep(0.75, 0.82, noise2(world * 22.0)) * 0.3;
           let grass = smoothstep(0.5, 0.75, fbm2(world * 0.6 + 3.0, 4)) * smoothstep(1.4, 2.6, length(world - vec2f(0.0, FIRE)));
           var albedo = mix(vec3f(0.04, 0.028, 0.018), vec3f(0.085, 0.055, 0.032), dirt) + vec3f(0.02, 0.010, 0.004) * needles + vec3f(0.05, 0.045, 0.04) * pebbles;
           albedo = mix(albedo, vec3f(0.035, 0.05, 0.018) * (0.6 + 0.8 * fbm2(world * 20.0, 3)), grass);
           let ring = abs(away - 0.95);
           let angle = atan2(world.y - fire.y, world.x - fire.x);
           let stone = smoothstep(0.17, 0.12, ring) * step(0.25, fract(angle * 2.6) );
           albedo = mix(albedo, vec3f(0.16, 0.15, 0.14) * (0.7 + 0.5 * fbm2(world * 6.0, 3)), stone);
           let warm = light * 2.8 / (1.0 + away * away * 2.2);
           let ash = smoothstep(0.7, 0.3, away) * 0.5;
           return albedo * (vec3f(1.0, 0.55, 0.25) * warm + vec3f(0.01, 0.015, 0.03)) + vec3f(0.6, 0.18, 0.04) * ash * light * 0.3;
       }

       fn sky(uv: vec2f) -> vec3f {
           var color = mix(vec3f(0.006, 0.010, 0.022), vec3f(0.001, 0.002, 0.006), smoothstep(HORIZON, 0.5, uv.y));
           let grid = uv * vec2f(220.0, 220.0);
           let star = pow(cell2(floor(grid)), 80.0) * exp(-dot(fract(grid) - 0.5, fract(grid) - 0.5) * 30.0);
           return color + vec3f(0.8, 0.85, 1.0) * star * smoothstep(0.25, 0.45, uv.y) * 2.0;
       }

       fn forest(uv: vec2f, light: f32, behind: vec3f) -> vec3f {
           var color = behind;
           for (var layer = 2; layer >= 0; layer--) {
               let depth = 7.0 + f32(layer) * 5.0;
               let spread = depth * 0.9;
               let x = uv.x * spread + f32(layer) * 17.0;
               let cell = floor(x);
               let local = (fract(x) - 0.5 - (cell2(vec2f(cell, f32(layer))) - 0.5) * 0.1) / spread;
               let present = step(0.22, cell2(vec2f(cell, 20.0 + f32(layer))));
               let bottom = HORIZON - EYE / depth;
               let top = HORIZON + (0.30 + 0.35 * cell2(vec2f(cell, 30.0 + f32(layer)))) * 8.0 / depth;
               let rise = (uv.y - bottom) / (top - bottom);
               let tiers = 0.78 + 0.22 * pow(0.5 + 0.5 * sin(rise * 46.0 + cell * 3.0), 2.0);
               let ragged = 0.9 + 0.2 * noise2(vec2f(local * 900.0, uv.y * 300.0));
               let reach = (1.0 - rise) * 0.30 / spread * tiers * ragged;
               let pixel = 1.5 / frame.size.y;
               let trunk = smoothstep(0.006 * 8.0 / depth + pixel, 0.006 * 8.0 / depth, abs(local)) * step(rise, 0.25);
               let needles = smoothstep(reach + pixel, reach, abs(local)) * step(0.10, rise);
               let solid = present * max(trunk, needles) * step(0.0, rise) * step(rise, 1.0);
               if (solid > 0.0) {
                   let warmth = light / (1.0 + pow(depth - FIRE, 2.0) * 0.15) * pow(max(1.0 - rise * 1.3, 0.0), 2.0);
                   let texture = 0.4 + 0.6 * fbm2(vec2f(local * 400.0, uv.y * 160.0), 4);
                   let fog = vec3f(0.004, 0.006, 0.011) * f32(layer);
                   let tree = vec3f(0.0015, 0.002, 0.002) + fog + vec3f(1.0, 0.42, 0.14) * warmth * 0.035 * texture;
                   color = mix(color, tree, solid);
               }
           }
           return color;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let light = flicker();
           var color: vec3f;
           if (uv.y < HORIZON) {
               color = ground(uv, light);
           } else {
               color = sky(uv);
           }
           color = forest(uv, light, color);
           let base = vec2f(0.0, HORIZON - EYE / FIRE);
           let scale = 0.75;
           let p = (uv - base) / scale;
           let fire = flames(p - vec2f(0.0, -0.06)) * smoothstep(-0.12, 0.02, p.y);
           color += heat(fire * 0.7) * smoothstep(0.0, 0.3, fire) * 0.85;
           let wood = logs(p, light);
           color = mix(color, wood.rgb, wood.a);
           let glow = exp(-length((p - vec2f(0.0, 0.35)) * vec2f(1.0, 0.8)) * 3.5);
           color += vec3f(1.0, 0.40, 0.10) * glow * 0.12 * light;
           let rising = vec2f(p.x * 1.2 - frame.time * 0.08, p.y * 0.8 - frame.time * 0.35);
           let smoke = smoothstep(0.45, 0.85, fbm2(rising, 5)) * smoothstep(0.9, 1.6, p.y) * smoothstep(3.5, 1.6, p.y) * exp(-abs(p.x - (p.y - 1.0) * 0.25) * 2.5);
           color = mix(color, vec3f(0.05, 0.045, 0.045) + vec3f(0.08, 0.035, 0.01) * light * exp(-(p.y - 1.0) * 1.5), smoke * 0.45);
           for (var i = 0; i < 70; i++) {
               let s = f32(i);
               let life = 2.4 + 3.0 * cell2(vec2f(s, 1.0));
               let age = fract(frame.time / life + cell2(vec2f(s, 2.0)));
               let start = vec2f((cell2(vec2f(s, 3.0)) - 0.5) * 0.45, 0.15);
               let wander = vec2f(sin(age * 9.0 + s) * 0.08 + (noise2(vec2f(s, frame.time * 0.6)) - 0.5) * 0.6 * age, age * age * 0.6 + age * 2.0);
               let spark = start + wander;
               let offset = p - spark;
               let size = 0.006 + 0.006 * cell2(vec2f(s, 4.0));
               let streak = vec2f(offset.x, offset.y * 0.45);
               let bright = (1.0 - age) * (0.5 + 0.5 * sin(frame.time * 13.0 + s * 3.0));
               color += heat(0.9 - age * 0.4) * exp(-dot(streak, streak) / (size * size)) * bright * 1.6;
           }
           let edge = length(uv * vec2f(0.8, 1.0));
           color *= 1.0 - smoothstep(0.45, 1.1, edge) * 0.7;
           return shown(color, at);
       }
       """))
