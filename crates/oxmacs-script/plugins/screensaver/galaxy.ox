(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const TILT: f32 = 1.05;
       const ARMS: f32 = 2.0;
       const PITCH: f32 = 0.30;

       fn background(uv: vec2f) -> vec3f {
           var color = vec3f(0.0015, 0.0018, 0.003);
           for (var layer = 0; layer < 3; layer++) {
               let scale = 90.0 + f32(layer) * 110.0;
               let grid = uv * scale + f32(layer) * 13.0;
               let cell = floor(grid);
               let spot = cell22(cell);
               let offset = fract(grid) - spot;
               let bright = pow(cell2(cell + 7.0), 28.0 - f32(layer) * 6.0);
               let tint = mix(vec3f(0.65, 0.75, 1.0), vec3f(1.0, 0.82, 0.6), cell2(cell + 3.0));
               color += tint * bright * exp(-dot(offset, offset) * scale * 6.0) * 2.0;
           }
           let far = uv * 6.0 + 30.0;
           let fuzz = exp(-length(fract(far) - 0.5) * 30.0) * step(0.93, cell2(floor(far)));
           color += vec3f(0.6, 0.55, 0.5) * fuzz * 0.05;
           return color;
       }

       fn disk(p: vec2f, t: f32) -> vec4f {
           let r = length(p);
           let theta = atan2(p.y, p.x);
           let spin = t * 0.04 / (0.3 + r);
           let winding = log(max(r, 0.02)) / PITCH;
           let arm = cos(ARMS * (theta - winding + spin));
           let arms = pow(0.5 + 0.5 * arm, 3.0);
           let swirl = rotation(spin * 0.5) * p;
           let clumps = fbm2(swirl * 9.0, 5);
           let lane = pow(0.5 + 0.5 * cos(ARMS * (theta - winding + spin) - 0.65), 6.0);
           let disk_light = exp(-r * 3.2);
           let bulge = exp(-pow(r / 0.07, 0.8)) * 2.5 + exp(-r * 12.0) * 0.8;
           let stars = disk_light * (0.18 + 1.3 * arms * (0.5 + clumps));
           let young = disk_light * arms * smoothstep(0.62, 0.8, fbm2(swirl * 22.0 + 3.0, 4)) * 2.2;
           let dust = smoothstep(0.35, 0.75, fbm2(swirl * 14.0 + 9.0, 5)) * lane * smoothstep(0.06, 0.2, r) * smoothstep(0.9, 0.35, r);
           var color = vec3f(1.0, 0.86, 0.66) * bulge + vec3f(0.95, 0.88, 0.80) * stars + vec3f(0.55, 0.70, 1.0) * young;
           color += vec3f(1.0, 0.35, 0.55) * young * smoothstep(0.7, 0.85, fbm2(swirl * 30.0, 3)) * 0.8;
           return vec4f(color, dust);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           var color = background(uv);
           let view = rotation(0.45) * uv;
           let plane = vec2f(view.x, view.y / cos(TILT)) * 1.7;
           let found = disk(plane, frame.time);
           let r = length(plane);
           let fade = smoothstep(1.3, 0.6, r);
           let near_side = smoothstep(0.0, -0.2, plane.y);
           let absorbed = found.a * (0.55 + 0.4 * near_side);
           color = color * (1.0 - fade * 0.8) + found.rgb * fade * 0.9;
           color *= 1.0 - absorbed * 0.85;
           let halo = exp(-length(uv) * 4.0) * 0.05;
           color += vec3f(0.9, 0.8, 0.7) * halo;
           return shown(color * 1.1, at);
       }
       """))
