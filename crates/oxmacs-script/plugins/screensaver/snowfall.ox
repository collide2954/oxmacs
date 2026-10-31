(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const HORIZON: f32 = 0.02;

       fn hill(x: f32, layer: f32) -> f32 {
           return HORIZON - 0.06 * layer + 0.05 * fbm2(vec2f(x * (1.2 + layer), layer * 7.0), 4) - 0.02;
       }

       fn pine(uv: vec2f, x: f32, base: f32, tall: f32) -> vec2f {
           let rise = (uv.y - base) / tall;
           if (rise < 0.0 || rise > 1.0) {
               return vec2f(0.0);
           }
           let tiers = 0.75 + 0.25 * pow(0.5 + 0.5 * sin(rise * 38.0), 1.5);
           let reach = (1.0 - rise) * tall * 0.33 * tiers;
           let offset = abs(uv.x - x);
           let pixel = 1.5 / frame.size.y;
           let inside = smoothstep(reach + pixel, reach, offset) * step(0.06, rise);
           let trunk = smoothstep(tall * 0.03 + pixel, tall * 0.03, offset) * step(rise, 0.1);
           let clumps = fbm2(uv * vec2f(160.0, 110.0) + x * 9.0, 4);
           let snow = smoothstep(0.50, 0.85, 0.45 + 0.45 * sin(rise * 38.0 + 1.3) + (clumps - 0.5) * 0.9);
           return vec2f(max(inside, trunk), snow * inside);
       }

       fn cabin(uv: vec2f) -> vec4f {
           let center = vec2f(-0.38, hill(-0.38, 1.0) - 0.012);
           let p = (uv - center) * 0.6;
           let body = step(abs(p.x), 0.05) * step(0.0, p.y) * step(p.y, 0.04);
           let roof = step(abs(p.x), 0.062 - (p.y - 0.04) * 1.1) * step(0.04, p.y) * step(p.y, 0.095);
           let window = step(abs(p.x - 0.018), 0.011) * step(abs(p.y - 0.02), 0.009);
           let door = step(abs(p.x + 0.022), 0.008) * step(p.y, 0.028) * step(0.0, p.y);
           let wall = vec3f(0.035, 0.03, 0.035);
           let roof_snow = vec3f(0.55, 0.58, 0.68) * (0.8 + 0.2 * noise2(p * 300.0));
           var color = wall * body + roof_snow * roof;
           color = mix(color, vec3f(1.6, 0.85, 0.35) * (0.9 + 0.1 * sin(frame.time * 3.0)), window + door * 0.4);
           let glow = exp(-length(p - vec2f(0.018, 0.02)) * 22.0) * 0.35;
           return vec4f(color, max(body, roof)) + vec4f(vec3f(1.0, 0.6, 0.25) * glow, 0.0);
       }

       fn flakes(uv: vec2f, layer: i32) -> f32 {
           let depth = 1.0 + f32(layer) * 1.4;
           let scale = 9.0 * depth;
           let wind = vec2f(frame.time * 0.05 * (1.0 + sin(frame.time * 0.1)) / depth, frame.time * 0.16 / depth * 1.4);
           let grid = (uv + wind) * scale + f32(layer) * 3.7;
           let cell = floor(grid);
           var sum = 0.0;
           for (var k = 0; k < 2; k++) {
               let seed = cell + f32(k) * 53.0;
               let spot = cell22(seed);
               let sway = vec2f(sin(frame.time * (0.6 + spot.x) + spot.y * 9.0) * 0.18, 0.0);
               let offset = fract(grid) - spot - sway;
               let focus = abs(depth - 3.0) * 0.035 + 0.02;
               let size = 0.025 + 0.035 * cell2(seed + 2.0);
               let d = length(offset);
               let sparse = step(0.45 + 0.3 / depth, cell2(seed + 5.0));
               sum += smoothstep(size + focus, size - focus * 0.5, d) * sparse * (0.35 + 0.4 / depth) / (1.0 + focus * 12.0);
           }
           return sum;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let height = uv.y - HORIZON;
           var color = mix(vec3f(0.18, 0.20, 0.27), vec3f(0.045, 0.055, 0.09), smoothstep(-0.05, 0.5, height));
           color += vec3f(0.12, 0.08, 0.06) * exp(-abs(height) * 10.0) * 0.4;
           for (var layer = 3; layer >= 1; layer--) {
               let l = f32(layer);
               let ridge = hill(uv.x, l);
               let haze = vec3f(0.14, 0.15, 0.20) * (l - 0.6) * 0.35;
               if (uv.y < ridge) {
                   let shade_snow = 0.25 + 0.3 * smoothstep(ridge - 0.2, ridge, uv.y);
                   color = vec3f(0.28, 0.30, 0.38) * shade_snow * (1.0 / l) + haze;
               }
               let nearest = floor((uv.x - l * 0.05) / (0.22 / l) + 7.5);
               for (var k = -1; k <= 1; k++) {
                   let t = nearest + f32(k);
                   let x = (t - 7.0) * (0.22 / l) + (cell2(vec2f(t, l)) - 0.5) * 0.12 / l + l * 0.05;
                   let tall = (0.10 + 0.08 * cell2(vec2f(t, l + 9.0))) * 1.6 / l;
                   let tree = pine(uv, x, hill(x, l) - 0.01 / l, tall);
                   if (tree.x > 0.0) {
                       let dark = vec3f(0.012, 0.018, 0.020) + haze * 0.8;
                       let white = vec3f(0.35, 0.37, 0.45) / l + haze;
                       color = mix(color, mix(dark, white, tree.y), tree.x);
                   }
               }
               if (layer == 1) {
                   let house = cabin(uv);
                   color = mix(color, house.rgb, house.a) + house.rgb * (1.0 - house.a);
               }
           }
           if (uv.y < hill(uv.x, 0.4) - 0.14) {
               let drift = fbm2(vec2f(uv.x * 4.0, uv.y * 12.0), 4);
               color = vec3f(0.30, 0.32, 0.40) * (0.55 + 0.3 * drift) + vec3f(0.25, 0.14, 0.06) * exp(-length(uv - vec2f(-0.38, -0.2)) * 3.0) * 0.15;
           }
           var snow = 0.0;
           for (var layer = 0; layer < 4; layer++) {
               snow += flakes(uv, layer);
           }
           color += vec3f(0.80, 0.82, 0.90) * snow * 0.55;
           color *= 1.0 - smoothstep(0.6, 1.3, length(uv * vec2f(0.8, 1.0))) * 0.55;
           return shown(color * 1.1, at);
       }
       """))
