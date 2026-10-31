(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const RADIUS: f32 = 2.0;
       const GAP: f32 = 7.0;
       const SPEED: f32 = 7.0;

       fn path(z: f32) -> vec2f {
           return vec2f(sin(z * 0.06) * 3.2 + sin(z * 0.023) * 2.0, cos(z * 0.05) * 2.2);
       }

       fn ribs(p: vec3f) -> f32 {
           let c = p.xy - path(p.z);
           let angle = atan2(c.y, c.x);
           let seam = abs(fract(angle * 10.0 / TAU) - 0.5);
           let band = abs(fract(p.z / 1.75) - 0.5);
           return smoothstep(0.02, 0.06, seam) * smoothstep(0.02, 0.05, band);
       }

       fn tube(p: vec3f) -> f32 {
           let c = p.xy - path(p.z);
           let floor_cut = c.y + 1.45;
           return min(RADIUS - length(c) + (1.0 - ribs(p)) * 0.035, floor_cut);
       }

       fn ring_light(z: f32) -> f32 {
           let local = fract(z / GAP);
           return exp(-pow(min(local, 1.0 - local) * GAP * 3.0, 2.0));
       }

       fn surface_normal(p: vec3f) -> vec3f {
           let e = 0.004;
           return normalize(vec3f(
               tube(p + vec3f(e, 0.0, 0.0)) - tube(p - vec3f(e, 0.0, 0.0)),
               tube(p + vec3f(0.0, e, 0.0)) - tube(p - vec3f(0.0, e, 0.0)),
               tube(p + vec3f(0.0, 0.0, e)) - tube(p - vec3f(0.0, 0.0, e))));
       }

       fn march(origin: vec3f, ray: vec3f) -> vec4f {
           var t = 0.02;
           var haze = 0.0;
           for (var i = 0; i < 110; i++) {
               let p = origin + ray * t;
               let d = tube(p);
               haze += ring_light(p.z) * 0.012 * min(d, 0.6);
               if (d < 0.001 * t) {
                   break;
               }
               t += d * 0.85;
               if (t > 80.0) {
                   break;
               }
           }
           return vec4f(t, haze, 0.0, 0.0);
       }

       fn lit(p: vec3f, normal: vec3f, ray: vec3f) -> vec3f {
           let c = p.xy - path(p.z);
           let angle = atan2(c.y, c.x);
           let panel = ribs(p);
           let grime = fbm2(vec2f(angle * 3.0, p.z * 0.6), 4);
           let floor_area = step(c.y, -1.44);
           var albedo = mix(vec3f(0.035, 0.040, 0.048), vec3f(0.11, 0.12, 0.13), panel) * (0.6 + 0.6 * grime);
           albedo = mix(albedo, vec3f(0.03, 0.03, 0.035) * (0.6 + 0.6 * grime), floor_area);
           var light = vec3f(0.0);
           let nearest = round(p.z / GAP) * GAP;
           for (var k = -1; k <= 1; k++) {
               let z = nearest + f32(k) * GAP;
               let lamp = vec3f(path(z) + vec2f(0.0, 1.6), z);
               let to = lamp - p;
               let d = length(to);
               let l = to / d;
               let hue = select(vec3f(0.25, 0.75, 1.0), vec3f(1.0, 0.55, 0.2), fract(z / (GAP * 4.0)) > 0.5);
               let diffuse = max(dot(normal, l), 0.0);
               let spec = pow(max(dot(reflect(ray, normal), l), 0.0), 40.0);
               light += hue * (diffuse * albedo * 5.0 + spec * 1.2 * (0.3 + 0.7 * floor_area)) / (1.0 + d * d * 0.6);
           }
           let emitter = ring_light(p.z) * step(0.0, c.y);
           let hue = select(vec3f(0.25, 0.75, 1.0), vec3f(1.0, 0.55, 0.2), fract(round(p.z / GAP) / 4.0) > 0.5);
           return light + hue * emitter * 3.0;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let z = frame.time * SPEED;
           let origin = vec3f(path(z), z);
           let ahead = vec3f(path(z + 4.0), z + 4.0);
           let forward = normalize(ahead - origin);
           let roll = sin(frame.time * 0.13) * 0.25;
           let side = normalize(cross(vec3f(sin(roll), cos(roll), 0.0), forward));
           let up = cross(forward, side);
           let ray = normalize(forward + side * uv.x * 1.2 + up * uv.y * 1.2);
           let hit = march(origin, ray);
           let p = origin + ray * hit.x;
           let normal = surface_normal(p);
           var color = lit(p, normal, ray);
           let bounce = reflect(ray, normal);
           if (p.y - path(p.z).y < -1.43) {
               let second = march(p + normal * 0.02, bounce);
               let q = p + normal * 0.02 + bounce * second.x;
               color += lit(q, surface_normal(q), bounce) * 0.25;
           }
           let fog = 1.0 - exp(-hit.x * 0.045);
           color = mix(color, vec3f(0.02, 0.03, 0.05), fog);
           color += vec3f(0.35, 0.55, 0.8) * hit.y;
           return shown(color, at);
       }
       """))
