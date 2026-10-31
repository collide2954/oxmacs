(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       struct Flow {
           height: f32,
           tint: f32,
       }

       fn flow(p: vec2f) -> Flow {
           let t = frame.time * 0.06;
           let q = vec2f(fbm2(p + vec2f(0.0, t), 5), fbm2(p + vec2f(5.2, 1.3 - t), 5));
           let r = vec2f(fbm2(p + 3.5 * q + vec2f(1.7, 9.2) + t * 0.7, 5), fbm2(p + 3.5 * q + vec2f(8.3, 2.8) - t * 0.5, 5));
           return Flow(fbm2(p + 3.0 * r, 4), r.x * 0.8 + q.y * 0.6);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           let p = uv * 2.2 + vec2f(frame.time * 0.015, 0.0);
           let here = flow(p);
           let e = 0.004;
           let dx = flow(p + vec2f(e, 0.0)).height - here.height;
           let dy = flow(p + vec2f(0.0, e)).height - here.height;
           let normal = normalize(vec3f(-dx / e * 0.03, -dy / e * 0.03, 1.0));
           let hue = here.tint * 0.8 + here.height * 0.6 + frame.time * 0.008;
           let albedo = palette(hue, vec3f(0.50, 0.46, 0.52), vec3f(0.50, 0.44, 0.46), vec3f(1.0, 1.0, 1.0), vec3f(0.00, 0.15, 0.35));
           let deep = palette(hue + 0.3, vec3f(0.05, 0.08, 0.18), vec3f(0.05, 0.08, 0.14), vec3f(1.0, 1.0, 1.0), vec3f(0.5, 0.6, 0.7));
           let key = normalize(vec3f(-0.5, 0.6, 0.65));
           let view = vec3f(0.0, 0.0, 1.0);
           let diffuse = max(dot(normal, key), 0.0);
           let half_way = normalize(key + view);
           let gloss = pow(max(dot(normal, half_way), 0.0), 90.0) * 1.6 + pow(max(dot(normal, half_way), 0.0), 12.0) * 0.15;
           let fresnel = pow(1.0 - max(dot(normal, view), 0.0), 3.0);
           let depth = smoothstep(0.2, 0.8, here.height);
           var color = mix(deep * 0.4, albedo * albedo * 1.3, depth) * (0.3 + 0.9 * diffuse);
           color += vec3f(1.0, 0.95, 0.9) * gloss + mix(albedo, vec3f(1.0), 0.5) * fresnel * 0.35;
           color *= 1.0 - smoothstep(0.5, 1.1, length(uv)) * 0.45;
           return shown(color * 0.9, at);
       }
       """))
