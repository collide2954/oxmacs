(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const MIRRORS: f32 = 6.0;
       const PIECES: i32 = 40;

       fn jewel(pick: f32) -> vec3f {
           let tints = array<vec3f, 8>(
               vec3f(0.95, 0.15, 0.20), vec3f(1.00, 0.55, 0.10), vec3f(1.00, 0.85, 0.20), vec3f(0.20, 0.75, 0.35),
               vec3f(0.15, 0.55, 0.95), vec3f(0.55, 0.25, 0.90), vec3f(0.95, 0.35, 0.70), vec3f(0.20, 0.85, 0.85));
           return tints[i32(pick * 7.999)];
       }

       fn capsule(p: vec2f, a: vec2f, b: vec2f, r: f32) -> f32 {
           let pa = p - a;
           let ba = b - a;
           let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
           return length(pa - ba * h) - r;
       }

       fn piece(p: vec2f, i: i32) -> vec2f {
           let s = f32(i);
           let wedge = PI / MIRRORS;
           let drift = frame.time * (0.006 + 0.01 * cell2(vec2f(s, 7.0)));
           let angle = fract(cell2(vec2f(s, 1.0)) + drift) * (wedge + 0.3) - 0.15;
           let reach = 0.03 + 0.46 * sqrt(fract(cell2(vec2f(s, 8.0)) + drift * 0.7));
           let home = vec2f(cos(angle), sin(angle)) * reach;
           let spin = frame.time * (0.05 + 0.08 * cell2(vec2f(s, 2.0))) * select(-1.0, 1.0, cell2(vec2f(s, 3.0)) > 0.5);
           let tumble = home + vec2f(sin(frame.time * 0.11 + s), cos(frame.time * 0.09 + s * 1.3)) * 0.01;
           let local = rotation(spin + s) * (p - tumble);
           let size = 0.015 + 0.035 * cell2(vec2f(s, 4.0));
           let kind = cell2(vec2f(s, 5.0));
           var d: f32;
           if (kind < 0.45) {
               d = length(local) - size;
           } else if (kind < 0.75) {
               d = capsule(local, vec2f(-size * 1.6, 0.0), vec2f(size * 1.6, 0.0), size * 0.35);
           } else {
               let q = abs(local);
               d = max(q.x * 0.866 + local.y * 0.5, -local.y) - size * 0.6;
           }
           return vec2f(d, size);
       }

       fn chamber(p: vec2f) -> vec3f {
           var light = vec3f(0.80, 0.77, 0.70) * (0.8 + 0.2 * fbm2(p * 9.0, 3));
           for (var i = 0; i < PIECES; i++) {
               let found = piece(p, i);
               let d = found.x;
               if (d < 0.004) {
                   let pixel = 2.0 / frame.size.y;
                   let cover = smoothstep(pixel, -pixel, d);
                   let tint = jewel(cell2(vec2f(f32(i), 6.0)));
                   let depth = clamp(-d / found.y, 0.0, 1.0);
                   let rim = smoothstep(0.0, 0.35, depth);
                   let filtered = light * mix(vec3f(1.0), tint * tint, 0.7 + 0.3 * rim);
                   let edge = (1.0 - rim) * 0.55;
                   let shine = pow(1.0 - abs(depth - 0.3) * 3.0, 6.0) * 0.25;
                   light = mix(light, filtered * (1.0 - edge) + vec3f(shine), cover);
               }
           }
           return light;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           let r = length(uv);
           var angle = atan2(uv.y, uv.x) + frame.time * 0.02;
           let slice = PI / MIRRORS;
           angle = abs((angle % (2.0 * slice) + 2.0 * slice) % (2.0 * slice) - slice);
           let folded = vec2f(cos(angle), sin(angle)) * r;
           var color = chamber(folded * 0.95);
           let seam = smoothstep(0.004, 0.0, abs(angle - slice) * r) + smoothstep(0.004, 0.0, angle * r);
           color *= 1.0 - seam * 0.15;
           let tube = smoothstep(0.47, 0.455, r);
           color = color * tube * (1.0 - smoothstep(0.30, 0.47, r) * 0.35);
           color += vec3f(0.05, 0.045, 0.04) * smoothstep(0.44, 0.47, r) * smoothstep(0.50, 0.47, r);
           return shown(color * 0.95, at);
       }
       """))
