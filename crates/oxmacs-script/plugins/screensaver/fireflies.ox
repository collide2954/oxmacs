(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const HORIZON: f32 = -0.08;

       fn sky(uv: vec2f) -> vec3f {
           let height = uv.y - HORIZON;
           var color = mix(vec3f(0.11, 0.10, 0.16), vec3f(0.012, 0.022, 0.06), smoothstep(0.0, 0.55, height));
           color += vec3f(0.22, 0.10, 0.06) * exp(-height * 9.0) * smoothstep(-0.9, 0.2, uv.x) * 0.6;
           let grid = uv * 260.0;
           let star = pow(cell2(floor(grid)), 90.0) * exp(-dot(fract(grid) - 0.5, fract(grid) - 0.5) * 40.0);
           color += vec3f(0.8, 0.85, 1.0) * star * smoothstep(0.15, 0.45, height) * 1.5;
           let moon = length(uv - vec2f(0.52, 0.30));
           color += vec3f(0.9, 0.88, 0.8) * smoothstep(0.034, 0.031, moon) * 1.4 + vec3f(0.25, 0.25, 0.3) * exp(-moon * 9.0) * 0.18;
           return color;
       }

       fn treeline(x: f32) -> f32 {
           var top = HORIZON + 0.015 + fbm2(vec2f(x * 2.0, 1.0), 4) * 0.05;
           for (var k = 0; k < 2; k++) {
               let scale = 16.0 + f32(k) * 14.0;
               let cell = floor(x * scale);
               let local = fract(x * scale) - 0.5;
               let tall = (0.04 + 0.10 * cell2(vec2f(cell, f32(k)))) * smoothstep(0.0, 0.6, abs(x - 0.1) + 0.15);
               let dome = sqrt(max(0.25 - local * local, 0.0)) * 2.0 * tall;
               let ragged = 0.004 * noise2(vec2f(x * 400.0, f32(k)));
               top = max(top, HORIZON + 0.01 + dome + ragged);
           }
           return top;
       }

       fn grass(uv: vec2f) -> f32 {
           var cover = 0.0;
           for (var layer = 0; layer < 3; layer++) {
               let scale = 60.0 + f32(layer) * 50.0;
               let x = uv.x * scale + f32(layer) * 7.0;
               let blade = fract(x) - 0.5;
               let cell = floor(x);
               let tall = (0.10 + 0.14 * cell2(vec2f(cell, f32(layer)))) / (1.0 + f32(layer) * 0.8);
               let base = -0.50 + f32(layer) * 0.12;
               let lean = (cell2(vec2f(cell, 9.0)) - 0.5) * 2.0 + sin(frame.time * 0.6 + cell * 0.3) * 0.25;
               let rise = (uv.y - base) / tall;
               let width = 0.18 * (1.0 - rise);
               let bent = blade - lean * rise * rise * 0.8;
               if (rise > 0.0 && rise < 1.0 && abs(bent) < width) {
                   cover = max(cover, 1.0 - f32(layer) * 0.15);
               }
           }
           return cover;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           var color = sky(uv);
           if (uv.y < treeline(uv.x)) {
               color = vec3f(0.006, 0.009, 0.010);
           }
           let meadow = smoothstep(HORIZON + 0.01, HORIZON - 0.05, uv.y);
           color = mix(color, vec3f(0.010, 0.016, 0.014) + vec3f(0.02, 0.025, 0.03) * smoothstep(-0.5, HORIZON, uv.y), meadow);
           let mist = exp(-abs(uv.y - HORIZON + 0.02) * 18.0) * (0.6 + 0.4 * fbm2(vec2f(uv.x * 3.0 - frame.time * 0.02, 1.0), 4));
           color += vec3f(0.07, 0.07, 0.10) * mist * 0.5;
           let together = smoothstep(10.0, 60.0, frame.time % 90.0);
           for (var i = 0; i < 70; i++) {
               let s = f32(i);
               let depth = 0.6 + 4.0 * cell2(vec2f(s, 1.0));
               let wander = vec2f(
                   noise2(vec2f(frame.time * 0.08, s * 3.1)) - 0.5,
                   noise2(vec2f(s * 2.3, frame.time * 0.07)) - 0.5);
               let home = vec2f((cell2(vec2f(s, 2.0)) - 0.5) * 3.2, HORIZON - 0.25 + 0.35 * cell2(vec2f(s, 3.0)));
               let place = vec3f(home + wander * 0.8, depth);
               let screen = vec2f(place.x / place.z, (place.y - 0.05) / place.z * 1.3 + HORIZON * (1.0 - 1.0 / place.z));
               let own = cell2(vec2f(s, 4.0)) * TAU;
               let phase = mix(own, 0.0, together) + frame.time * TAU / 2.6;
               let flash = pow(max(sin(phase), 0.0), 18.0) * step(0.15, cell2(vec2f(s, floor(frame.time / 2.6) + 5.0)) + together);
               let blur = abs(depth - 1.8) * 0.004 + 0.0012;
               let d = length(uv - screen);
               let glow = exp(-d * d / (blur * blur)) + exp(-d * 40.0 * depth) * 0.12;
               let tint = vec3f(0.75, 1.0, 0.25);
               color += tint * glow * flash * 1.6 / (0.6 + depth * 0.4);
               color += tint * exp(-d * 6.0) * flash * 0.012;
           }
           let blades = grass(uv);
           color = mix(color, vec3f(0.004, 0.006, 0.005), blades);
           color *= 1.0 - smoothstep(0.6, 1.3, length(uv * vec2f(0.8, 1.0))) * 0.6;
           return shown(color * 1.2, at);
       }
       """))
