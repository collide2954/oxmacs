(module
  (import (screensaver shading :as shading))
  (export space studio paper dusk deep))

(def space
  (str shading/library
       """
       fn stars(p: vec2f, scale: f32, rarity: f32) -> vec3f {
           let grid = p * scale;
           let cell = floor(grid);
           let offset = fract(grid) - 0.2 - 0.6 * cell22(cell);
           let tint = mix(vec3f(0.65, 0.75, 1.0), vec3f(1.0, 0.82, 0.62), cell2(cell + 5.0));
           return tint * pow(cell2(cell + 17.0), rarity) * exp(-dot(offset, offset) * scale * 3.0);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           let drift = uv + vec2f(frame.time * 0.002, 0.0);
           var color = vec3f(0.002, 0.003, 0.007);
           let cloud = fbm2(drift * 2.2 + 3.0, 6);
           let wisps = smoothstep(0.45, 0.85, cloud) * smoothstep(0.3, 0.9, fbm2(drift * 5.0 + 8.0, 5));
           color += mix(vec3f(0.05, 0.015, 0.06), vec3f(0.01, 0.04, 0.06), fbm2(drift * 1.3, 3)) * wisps * 0.9;
           color += stars(drift, 240.0, 26.0) * 2.0 + stars(drift + 9.0, 600.0, 14.0) * 0.6;
           return shown(color, at);
       }
       """))

(def studio
  (str shading/library
       """
       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           let floor_line = 0.30;
           var color = mix(vec3f(0.030, 0.032, 0.040), vec3f(0.008, 0.009, 0.012), smoothstep(-0.5, 0.3, -uv.y));
           let spot = exp(-length((uv - vec2f(0.0, -0.1)) * vec2f(0.8, 1.2)) * 2.4);
           color += vec3f(0.10, 0.10, 0.11) * spot * 0.5;
           if (uv.y > floor_line) {
               let depth = (uv.y - floor_line);
               let boards = 0.92 + 0.08 * noise2(vec2f(uv.x * 3.0 / (depth + 0.1), depth * 40.0));
               color = vec3f(0.018, 0.016, 0.016) * boards + vec3f(0.08, 0.075, 0.08) * spot * exp(-depth * 5.0) * 0.6;
           }
           color *= 1.0 - smoothstep(0.5, 1.2, length(uv * vec2f(0.8, 1.0))) * 0.7;
           return shown(color, at);
       }
       """))

(def paper
  (str shading/library
       """
       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           let fibers = fbm2(at * vec2f(0.9, 0.25), 4) * 0.5 + fbm2(at * vec2f(0.2, 1.1), 4) * 0.5;
           let grain = noise2(at * 1.7);
           var color = vec3f(0.80, 0.76, 0.66) * (0.94 + 0.05 * fibers + 0.025 * grain);
           let light = 1.0 - length(uv - vec2f(-0.3, -0.35)) * 0.35;
           color *= light;
           color *= 1.0 - smoothstep(0.55, 1.2, length(uv * vec2f(0.75, 1.0))) * 0.5;
           return shown(color * 0.75, at);
       }
       """))

(def dusk
  (str shading/library
       """
       fn hills(x: f32, layer: f32) -> f32 {
           let rolling = 0.07 * fbm2(vec2f(x * (1.2 + layer * 0.8) + layer * 3.0, layer * 5.0), 5);
           let swell = 0.04 * sin(x * (2.0 + layer) + layer * 2.0);
           return -0.04 - layer * 0.10 + rolling + swell;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let height = uv.y;
           var color = mix(vec3f(0.95, 0.55, 0.32), vec3f(0.10, 0.16, 0.36), smoothstep(-0.1, 0.5, height));
           color = mix(color, vec3f(0.03, 0.05, 0.14), smoothstep(0.35, 0.6, height));
           color += vec3f(1.0, 0.6, 0.3) * exp(-length(uv - vec2f(0.45, -0.02)) * 5.0) * 0.6;
           let clouds = smoothstep(0.5, 0.8, fbm2(vec2f(uv.x * 2.0 + frame.time * 0.005, uv.y * 7.0), 5)) * smoothstep(0.0, 0.25, height);
           color = mix(color, vec3f(0.55, 0.32, 0.30), clouds * 0.5);
           for (var layer = 0; layer < 3; layer++) {
               let l = f32(layer);
               let ridge = hills(uv.x, l);
               if (height < ridge) {
                   let far = vec3f(0.30, 0.24, 0.32);
                   let near = vec3f(0.035, 0.05, 0.035);
                   let base = mix(far, near, l * 0.5);
                   let rim = smoothstep(ridge - 0.03, ridge, height) * (1.0 - l * 0.4) * 0.15;
                   let texture = 0.85 + 0.15 * fbm2(vec2f(uv.x * 20.0, uv.y * 40.0) + l * 7.0, 4);
                   color = base * texture + vec3f(1.0, 0.55, 0.3) * rim;
               }
           }
           if (height < -0.32) {
               let grass = fbm2(vec2f(uv.x * 30.0, uv.y * 90.0), 4);
               color = vec3f(0.025, 0.04, 0.02) * (0.6 + 0.8 * grass);
           }
           return shown(color, at);
       }
       """))

(def deep
  (str shading/library
       """
       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           let swirl = fbm2(uv * 1.5 + vec2f(frame.time * 0.01, 0.0), 5);
           var color = mix(vec3f(0.006, 0.010, 0.022), vec3f(0.020, 0.018, 0.040), swirl);
           color *= 1.0 - smoothstep(0.4, 1.2, length(uv)) * 0.6;
           return shown(color, at);
       }
       """))
