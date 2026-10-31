(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       fn lights(uv: vec2f, focus: f32) -> vec3f {
           var color = mix(vec3f(0.012, 0.014, 0.024), vec3f(0.05, 0.035, 0.04), smoothstep(0.5, -0.5, uv.y));
           for (var layer = 0; layer < 3; layer++) {
               let scale = 3.0 + f32(layer) * 2.5;
               let grid = uv * scale + vec2f(f32(layer) * 7.3 + frame.time * 0.004 * f32(layer), 0.0);
               let cell = floor(grid);
               for (var j = -1; j <= 1; j++) {
                   for (var i = -1; i <= 1; i++) {
                       let seed = cell + vec2f(f32(i), f32(j));
                       let street = smoothstep(0.35, -0.35, (seed.y + 0.5) / scale);
                       if (cell2(seed + 5.0) > 0.25 + 0.55 * street) {
                           continue;
                       }
                       let spot = seed + cell22(seed);
                       let d = length(grid - spot);
                       let size = (0.18 + 0.25 * cell2(seed + 2.0)) * (1.0 - focus * 0.6);
                       let soft = 0.04 + (1.0 - focus) * 0.06;
                       let pick = cell2(seed + 9.0);
                       let tint = select(select(vec3f(1.0, 0.62, 0.25), vec3f(0.9, 0.25, 0.2), pick > 0.7), vec3f(0.55, 0.75, 1.0), pick < 0.25);
                       let flicker = 0.85 + 0.15 * sin(frame.time * (0.5 + pick) + pick * 20.0);
                       let disc = smoothstep(size, size - soft, d) * (0.85 + 0.15 * smoothstep(size * 0.6, size, d));
                       color += tint * disc * (0.30 / (1.0 + f32(layer) * 0.7)) * flicker;
                   }
               }
           }
           return color;
       }

       fn drops(uv: vec2f, t: f32) -> vec3f {
           let aspect = vec2f(1.0, 2.0);
           var grid = uv * vec2f(4.0, 2.0);
           let column = floor(grid.x);
           grid.y += t * 0.25 + cell2(vec2f(column, 1.0));
           let cell = floor(grid);
           let local = fract(grid) - 0.5;
           let seed = cell2(cell + 3.0);
           let cycle = fract(t * 0.18 + seed);
           let slide = -(sin(cycle * TAU + sin(cycle * TAU) * 0.5) * 0.45);
           let x = (seed - 0.5) * 0.6 + sin(cycle * 30.0) * 0.03 * (1.0 - cycle);
           let offset = (local - vec2f(x, slide)) * aspect;
           let main = smoothstep(0.11, 0.09, length(offset));
           let behind = local.y - slide;
           let trail_x = (local.x - x) * aspect.x;
           let trail = smoothstep(0.02, 0.0, abs(trail_x)) * smoothstep(-0.02, 0.02, behind) * smoothstep(0.5, slide, local.y) * 0.4;
           let beads = fract(local.y * 8.0) - 0.5;
           let bead_offset = vec2f(trail_x, beads * 0.125);
           let bead = smoothstep(0.045, 0.035, length(bead_offset)) * step(0.0, behind) * smoothstep(0.5, -0.2, local.y - slide);
           let cover = max(main, bead * 0.8);
           let bend = offset / 0.11 * main + bead_offset / 0.045 * bead * 0.5;
           return vec3f(bend * 0.1, max(cover, trail));
       }

       fn specks(uv: vec2f, t: f32) -> f32 {
           let grid = uv * 22.0;
           let cell = floor(grid);
           let spot = 0.25 + 0.5 * cell22(cell);
           let life = fract(t * 0.15 + cell2(cell + 7.0));
           let d = length(fract(grid) - spot);
           let size = 0.08 + 0.12 * cell2(cell + 4.0);
           return smoothstep(size, size * 0.7, d) * step(0.45, cell2(cell + 2.0)) * smoothstep(1.0, 0.85, life);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let t = frame.time;
           let a = drops(uv, t);
           let b = drops(uv * 1.7 + 3.3, t * 1.1);
           let c = specks(uv, t);
           let wet = max(max(a.z, b.z), c);
           let bend = a.xy + b.xy * 0.8;
           let sharp = lights(uv * 0.85 - bend * 5.0 + vec2f(0.0, 0.05), 0.55) * 1.7;
           let blurred = lights(uv, 0.0);
           let fog = 0.6;
           var color = mix(blurred * 0.8 + vec3f(0.035, 0.035, 0.045) * fog, sharp, smoothstep(0.05, 0.5, wet));
           let edge = smoothstep(0.15, 0.6, wet) * smoothstep(1.0, 0.6, wet);
           color *= 1.0 - edge * 0.15;
           color += blurred * smoothstep(0.0, -0.06, bend.y) * wet * 0.6;
           let normal = normalize(vec3f(bend * 9.0, 1.0));
           color += vec3f(1.0, 0.97, 0.95) * pow(max(dot(normal, normalize(vec3f(-0.4, 0.5, 0.8))), 0.0), 80.0) * wet * 0.6;
           color *= 1.0 - smoothstep(0.6, 1.3, length(uv * vec2f(0.8, 1.0))) * 0.4;
           return shown(color * 1.2, at);
       }
       """))
