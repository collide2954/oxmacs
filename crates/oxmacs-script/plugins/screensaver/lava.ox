(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const BOTTOM: f32 = -0.30;
       const TOP: f32 = 0.27;

       fn radius(y: f32) -> f32 {
           let t = clamp((y - BOTTOM) / (TOP - BOTTOM), 0.0, 1.0);
           return mix(0.115, 0.062, t) + 0.018 * sin(t * PI) ;
       }

       fn blob(i: i32) -> vec3f {
           let s = f32(i);
           let period = 26.0 + 14.0 * cell2(vec2f(s, 1.0));
           let phase = frame.time / period + cell2(vec2f(s, 2.0));
           let travel = 0.5 - 0.5 * cos(phase * TAU);
           let y = mix(BOTTOM + 0.04, TOP - 0.05, smoothstep(0.0, 1.0, travel));
           let x = (cell2(vec2f(s, 3.0)) - 0.5) * 0.9 * radius(y) + 0.012 * sin(frame.time * 0.3 + s * 2.0);
           let size = 0.012 + 0.017 * cell2(vec2f(s, 4.0));
           let stretch = 1.0 + 0.25 * abs(sin(phase * TAU));
           return vec3f(x, y, size * mix(1.0, 0.8, travel) * stretch);
       }

       fn field(p: vec2f) -> f32 {
           var sum = 0.0;
           for (var i = 0; i < 9; i++) {
               let b = blob(i);
               let d = p - b.xy;
               let squash = vec2f(d.x * 1.15, d.y * 0.85);
               sum += b.z * b.z / (dot(squash, squash) + 0.00005);
           }
           let pool = max(p.y - BOTTOM, 0.0) + 0.004 * sin(p.x * 60.0 + frame.time * 0.2);
           sum += 0.00025 / (pool * pool + 0.0001);
           return sum;
       }

       fn wall(uv: vec2f) -> vec3f {
           let glow = exp(-length((uv - vec2f(0.0, -0.05)) * vec2f(1.0, 1.3)) * 3.2);
           let paint = 0.9 + 0.1 * fbm2(uv * 40.0, 3);
           var color = vec3f(0.010, 0.008, 0.012) * paint + vec3f(0.55, 0.12, 0.05) * glow * 0.18 * paint;
           if (uv.y < -0.50) {
               let wood = fbm2(vec2f(uv.x * 3.0, uv.y * 60.0), 4);
               color = vec3f(0.020, 0.012, 0.008) * (0.6 + 0.6 * wood) + vec3f(0.6, 0.14, 0.05) * exp(-length(uv - vec2f(0.0, -0.5)) * 6.0) * 0.12;
           }
           return color;
       }

       fn metal(uv: vec2f, top: f32, bottom: f32, wide: f32, narrow: f32) -> vec4f {
           let t = clamp((uv.y - bottom) / (top - bottom), 0.0, 1.0);
           let r = mix(wide, narrow, t);
           if (uv.y < bottom || uv.y > top || abs(uv.x) > r) {
               return vec4f(0.0);
           }
           let across = uv.x / r;
           let curve = sqrt(max(1.0 - across * across, 0.0));
           let streak = exp(-pow((across + 0.45) * 6.0, 2.0)) * 0.9 + exp(-pow((across - 0.6) * 10.0, 2.0)) * 0.3;
           let brushed = 0.9 + 0.1 * noise2(vec2f(across * 3.0, uv.y * 800.0));
           let color = vec3f(0.05, 0.045, 0.045) * curve * brushed + vec3f(0.55, 0.50, 0.48) * streak * 0.35 + vec3f(0.5, 0.12, 0.04) * (1.0 - t) * 0.04;
           return vec4f(color, 1.0);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0) * 1.05;
           var color = wall(uv);
           let base = metal(uv, BOTTOM, -0.50, 0.17, 0.118);
           color = mix(color, base.rgb, base.a);
           let lid = metal(uv, 0.37, TOP, 0.058, 0.026);
           color = mix(color, lid.rgb, lid.a);
           let r = radius(uv.y);
           if (uv.y > BOTTOM && uv.y < TOP && abs(uv.x) < r) {
               let across = uv.x / r;
               let bend = vec2f(uv.x * (1.0 - 0.18 * across * across), uv.y);
               let light = exp(-(uv.y - BOTTOM) * 4.0);
               var inside = vec3f(0.07, 0.020, 0.10) * (0.30 + 1.2 * light) + vec3f(0.40, 0.08, 0.03) * light * 0.30;
               let f = field(bend);
               let e = 0.002;
               let lift = f / (1.0 + f);
               let gradient = vec2f(
                   field(bend + vec2f(e, 0.0)) / (1.0 + field(bend + vec2f(e, 0.0))) - lift,
                   field(bend + vec2f(0.0, e)) / (1.0 + field(bend + vec2f(0.0, e))) - lift) / e;
               let wax = smoothstep(0.95, 1.08, f);
               if (wax > 0.0) {
                   let normal = normalize(vec3f(-gradient * 0.02, 1.0));
                   let rim = 1.0 - smoothstep(1.0, 2.6, f);
                   let below = clamp(dot(normal, normalize(vec3f(0.0, -0.8, 0.6))), 0.0, 1.0);
                   let core = vec3f(0.60, 0.07, 0.015) * (0.30 + 0.9 * light + 0.6 * below);
                   let scatter = vec3f(1.0, 0.30, 0.05) * rim * (0.35 + 0.9 * light);
                   let shine = pow(clamp(dot(reflect(vec3f(0.0, 0.0, -1.0), normal), normalize(vec3f(-0.4, 0.5, 0.8))), 0.0, 1.0), 40.0) * 0.25;
                   inside = mix(inside, core + scatter + shine, wax);
               }
               let edge = pow(abs(across), 6.0);
               let highlight = exp(-pow((across + 0.55) * 7.0, 2.0)) * 0.10 + exp(-pow((across - 0.72) * 14.0, 2.0)) * 0.05;
               let glass = vec3f(0.6, 0.6, 0.7) * highlight + vec3f(0.03, 0.02, 0.04) * edge;
               let within = smoothstep(r, r - 1.5 / frame.size.y, abs(uv.x));
               color = mix(color, inside * (1.0 - edge * 0.5) + glass, within);
           }
           let spill = exp(-length((uv - vec2f(0.0, -0.52)) * vec2f(0.6, 3.0)) * 4.0);
           color += vec3f(0.6, 0.15, 0.05) * spill * 0.05;
           return shown(color * 1.3, at);
       }
       """))
