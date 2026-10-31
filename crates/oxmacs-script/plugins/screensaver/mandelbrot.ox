(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const DIVE: f32 = 50.0;
       const DEEPEST: f32 = 0.00025;

       fn destination(i: i32) -> vec2f {
           switch i {
               case 0: { return vec2f(-0.7436438870, 0.1318259042); }
               case 1: { return vec2f(-0.1010963638, 0.9562865108); }
               case 2: { return vec2f(0.3602404434, -0.6413130610); }
               default: { return vec2f(-1.2500656240, 0.0201199730); }
           }
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let cycle = floor(frame.time / DIVE);
           let local = frame.time - cycle * DIVE;
           let center = destination(i32(cycle) % 4);
           let depth = log(3.0 / DEEPEST) * smoothstep(0.0, DIVE - 4.0, local);
           let scale = 3.0 * exp(-depth);
           let turn = rotation(local * 0.02 + cycle);
           let c = center + turn * uv * scale;
           let most = i32(220.0 + depth * 60.0);
           var z = vec2f(0.0);
           var dz = vec2f(0.0);
           var n = 0;
           var escaped = false;
           for (var i = 0; i < 1400; i++) {
               if (i >= most) {
                   break;
               }
               dz = 2.0 * vec2f(z.x * dz.x - z.y * dz.y, z.x * dz.y + z.y * dz.x) + vec2f(1.0, 0.0);
               z = vec2f(z.x * z.x - z.y * z.y, z.x * z.y * 2.0) + c;
               if (dot(z, z) > 1024.0) {
                   escaped = true;
                   n = i;
                   break;
               }
           }
           var color = vec3f(0.0);
           if (escaped) {
               let r2 = dot(z, z);
               let smooth_n = f32(n) - log2(log2(r2)) + 4.0;
               let estimate = 0.5 * sqrt(r2 / dot(dz, dz)) * log(r2);
               let pixel = scale / frame.size.y;
               let edge = smoothstep(0.0, pixel * 3.0, estimate);
               let t = sqrt(smooth_n) * 0.22 + local * 0.004 + cycle * 0.37;
               let base = palette(t, vec3f(0.5, 0.42, 0.38), vec3f(0.5, 0.42, 0.40), vec3f(1.0, 1.0, 1.0), vec3f(0.62, 0.55, 0.45));
               let zdz = vec2f(z.x * dz.x + z.y * dz.y, z.y * dz.x - z.x * dz.y);
               let facing = normalize(zdz);
               let key = normalize(vec2f(0.6, 0.8));
               let relief = 0.55 + 0.45 * dot(facing, key);
               color = base * base * (0.25 + 0.95 * relief) * edge * 1.4;
               color += vec3f(1.0, 0.85, 0.6) * pow(relief, 12.0) * 0.25 * edge;
           } else {
               color = vec3f(0.002, 0.003, 0.008);
           }
           let fade = smoothstep(0.0, 1.5, local) * smoothstep(DIVE, DIVE - 2.5, local);
           return shown(color * fade, at);
       }
       """))
