(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const COUNT: i32 = 14;

       fn garden(uv: vec2f) -> vec3f {
           var color = mix(vec3f(0.25, 0.42, 0.66), vec3f(0.55, 0.70, 0.90), smoothstep(-0.2, 0.6, uv.y));
           color += vec3f(0.9, 0.75, 0.45) * exp(-length(uv - vec2f(-0.7, 0.55)) * 2.5) * 0.6;
           let leaves = fbm2(uv * vec2f(2.2, 3.0) + vec2f(0.0, frame.time * 0.01), 5);
           let foliage = smoothstep(0.40, 0.58, leaves + (0.05 - uv.y) * 0.9);
           let sunlit = fbm2(uv * 5.0 + 4.0, 4);
           let greens = mix(vec3f(0.04, 0.10, 0.025), vec3f(0.30, 0.48, 0.10), sunlit);
           color = mix(color, greens, foliage);
           let lawn = smoothstep(-0.25, -0.45, uv.y);
           color = mix(color, mix(vec3f(0.12, 0.25, 0.05), vec3f(0.35, 0.55, 0.12), fbm2(uv * vec2f(3.0, 12.0), 4)), lawn);
           for (var i = 0; i < 22; i++) {
               let s = f32(i);
               let spot = vec2f((cell2(vec2f(s, 1.0)) - 0.5) * 2.0, (cell2(vec2f(s, 2.0)) - 0.4) * 1.0);
               let d = length(uv - spot);
               let size = 0.03 + 0.06 * cell2(vec2f(s, 3.0));
               color += vec3f(1.0, 0.95, 0.75) * smoothstep(size, size * 0.9, d) * 0.22 * foliage;
           }
           return color;
       }

       fn film(thickness: f32, cosine: f32) -> vec3f {
           let path = 2.0 * 1.33 * thickness * cosine;
           let waves = vec3f(650.0, 532.0, 450.0);
           let phase = TAU * path / waves;
           return 0.5 - 0.5 * cos(phase);
       }

       fn bubble(i: i32) -> vec4f {
           let s = f32(i);
           let rise = 0.04 + 0.05 * cell2(vec2f(s, 4.0));
           let period = 3.0 / rise;
           let age = fract(frame.time / period + cell2(vec2f(s, 5.0)));
           let x = (cell2(vec2f(s, 6.0)) - 0.5) * 1.6 + sin(frame.time * 0.5 + s * 2.0) * 0.06 + age * (cell2(vec2f(s, 8.0)) - 0.5) * 0.4;
           let y = -0.75 + age * 1.6;
           let radius = 0.05 + 0.10 * cell2(vec2f(s, 7.0));
           let depth = 2.6 - 1.6 * s / f32(COUNT);
           return vec4f(x, y, radius / depth * 1.4, depth);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let blur = garden(uv * 0.9);
           var color = blur;
           for (var order = 0; order < COUNT; order++) {
               let b = bubble(order);
               let offset = (uv - b.xy) / b.z;
               let wobble = 1.0 + 0.04 * sin(atan2(offset.y, offset.x) * 3.0 + frame.time * 2.0 + f32(order));
               let r = length(offset) * wobble;
               if (r < 1.0) {
                   let z = sqrt(1.0 - r * r);
                   let normal = vec3f(offset / wobble, z);
                   let cosine = z;
                   let swirl = fbm2(offset * 1.5 + vec2f(frame.time * 0.15, -frame.time * 0.1) + f32(order) * 3.0, 4);
                   let thickness = mix(220.0, 900.0, swirl) * (0.7 + 0.5 * (1.0 - normal.y) * 0.5);
                   let fresnel = 0.04 + 0.96 * pow(1.0 - cosine, 4.0);
                   let iridescence = film(thickness, cosine) * (0.25 + fresnel * 1.4);
                   let behind = garden((b.xy + offset * b.z * 0.85) * 0.9);
                   let window = smoothstep(0.12, 0.08, length(normal.xy - vec2f(-0.38, 0.42))) * 1.4;
                   let sill = smoothstep(0.08, 0.05, length(normal.xy - vec2f(0.45, -0.5))) * 0.4;
                   let rim = smoothstep(0.80, 1.0, r) * 0.25;
                   let film_color = iridescence * 0.55 + vec3f(1.0) * (window + sill) + rim;
                   color = mix(color, behind, 0.6) * (1.0 - fresnel * 0.3) + film_color;
               }
           }
           color *= 1.0 - smoothstep(0.6, 1.3, length(uv * vec2f(0.8, 1.0))) * 0.4;
           return shown(color * 0.9, at);
       }
       """))
