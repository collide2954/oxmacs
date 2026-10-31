(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const DEPTH: f32 = 2.6;
       const DRAG: f32 = 0.42;
       const SUN: vec3f = vec3f(0.26, 0.10, 0.96);

       fn waves(position: vec2f, iterations: i32) -> f32 {
           var p = position;
           var k = 0.16;
           var weight = 1.0;
           var sum = 0.0;
           var total = 0.0;
           var angle = 0.9;
           for (var i = 0; i < iterations; i++) {
               let direction = vec2f(cos(angle), sin(angle));
               let phase = dot(direction, p) * k + frame.time * sqrt(9.81 * k);
               let wave = exp(sin(phase) - 1.0);
               sum += wave * weight;
               total += weight;
               p -= direction * wave * cos(phase) * weight * DRAG / k * 0.12;
               weight *= 0.79;
               k *= 1.19;
               angle += 2.39996;
           }
           return sum / total * DEPTH - DEPTH;
       }

       fn sea_normal(p: vec2f, e: f32, iterations: i32) -> vec3f {
           let h = waves(p, iterations);
           let x = waves(p + vec2f(e, 0.0), iterations);
           let z = waves(p + vec2f(0.0, e), iterations);
           return normalize(vec3f(h - x, e, h - z));
       }

       fn sun_direction() -> vec3f {
           return normalize(SUN);
       }

       fn sky(direction: vec3f, detail: i32) -> vec3f {
           let d = normalize(vec3f(direction.x, max(direction.y, 0.0), direction.z));
           let sun = sun_direction();
           let toward = max(dot(d, sun), 0.0);
           let height = d.y;
           let around = pow(max(dot(normalize(d.xz), normalize(sun.xz)), 0.0), 2.0);
           let zenith = vec3f(0.020, 0.070, 0.22);
           let middle = mix(vec3f(0.18, 0.26, 0.36), vec3f(0.62, 0.56, 0.42), around);
           let horizon = mix(vec3f(0.26, 0.30, 0.38), vec3f(1.0, 0.48, 0.17), around);
           var color = mix(horizon, middle, smoothstep(0.0, 0.12, height));
           color = mix(color, zenith, smoothstep(0.06, 0.55, height));
           color += vec3f(0.9, 0.42, 0.12) * (pow(toward, 6.0) * 0.30 + pow(toward, 48.0) * 0.7);
           color += vec3f(1.0, 0.62, 0.28) * pow(toward, 900.0) * 3.0;
           color += vec3f(1.0, 0.78, 0.45) * smoothstep(0.99988, 0.99993, toward) * 24.0;
           if (direction.y > 0.002) {
               let plane = d.xz / d.y * 1200.0 + vec2f(frame.time * 9.0, frame.time * 3.0);
               let shape = fbm2(plane * 0.0016, detail);
               let soft = f32(7 - detail) * 0.05;
               let cover = smoothstep(0.50 - soft, 0.70 + soft, shape) * smoothstep(0.0, 0.08, height);
               let thick = smoothstep(0.55, 0.9, shape);
               let rim = pow(toward, 4.0);
               let near = pow(toward, 1.5);
               let base = mix(vec3f(0.07, 0.075, 0.10), vec3f(0.26, 0.15, 0.12), near);
               let edge = vec3f(1.7, 0.70, 0.26) * (rim * 1.4 + near * 0.18) * (1.0 - thick * 0.75);
               let top = vec3f(0.30, 0.22, 0.26) * (1.0 - thick) * (1.0 - near) * smoothstep(0.1, 0.5, height);
               color = mix(color, base + edge + top, cover * 0.9);
           }
           return color;
       }

       fn haze(direction: vec3f) -> vec3f {
           return sky(vec3f(direction.x, 0.01, direction.z), 2);
       }

       fn trace(origin: vec3f, ray: vec3f) -> f32 {
           let top = -origin.y / ray.y;
           let bottom = (-DEPTH - origin.y) / ray.y;
           var near = top;
           var far = min(bottom, 4000.0);
           let steps = 72;
           let step = (far - near) / f32(steps);
           var t = near;
           var hit = far;
           for (var i = 0; i < steps; i++) {
               let p = origin + ray * t;
               if (p.y < waves(p.xz, 10)) {
                   hit = t;
                   near = t - step;
                   break;
               }
               t += step;
           }
           far = hit;
           for (var i = 0; i < 10; i++) {
               let middle = (near + far) * 0.5;
               let p = origin + ray * middle;
               if (p.y < waves(p.xz, 10)) {
                   far = middle;
               } else {
                   near = middle;
               }
           }
           return far;
       }

       fn camera() -> mat3x3f {
           let yaw = 0.18 + 0.05 * sin(frame.time * 0.03);
           let pitch = -0.035 + 0.006 * sin(frame.time * 0.31);
           let roll = 0.012 * sin(frame.time * 0.23);
           let forward = normalize(vec3f(sin(yaw), pitch, cos(yaw)));
           let side = normalize(cross(vec3f(sin(roll), cos(roll), 0.0), forward));
           let up = cross(forward, side);
           return mat3x3f(side, up, forward);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let travel = vec2f(frame.time * 1.2, frame.time * 3.0);
           let swell = waves(travel, 6);
           let origin = vec3f(travel.x, 3.0 + swell * 0.45, travel.y);
           let ray = normalize(camera() * vec3f(uv * 0.95, 1.0));
           let sun = sun_direction();
           if (ray.y >= 0.0) {
               return shown(sky(ray, 7) * 0.8, at);
           }
           let t = trace(origin, ray);
           let p = origin + ray * t;
           let distance = length(p - origin);
           let detail = i32(mix(34.0, 12.0, clamp(distance / 900.0, 0.0, 1.0)));
           var normal = sea_normal(p.xz, 0.02 + distance * 0.0015, detail);
           normal = normalize(mix(normal, vec3f(0.0, 1.0, 0.0), smoothstep(150.0, 3000.0, distance) * 0.85));
           let facing = max(dot(normal, -ray), 0.0);
           let fresnel = 0.02 + 0.98 * pow(1.0 - facing, 5.0);
           var bounced = reflect(ray, normal);
           bounced.y = abs(bounced.y);
           let reflected = sky(bounced, 3);
           let rise = clamp((p.y + DEPTH) / DEPTH, 0.0, 1.0);
           let deep = vec3f(0.004, 0.022, 0.040);
           let through = vec3f(0.04, 0.20, 0.17) * pow(rise, 2.5) * (0.35 + 1.4 * pow(max(dot(ray, sun), 0.0), 3.0));
           let body = deep + through * (0.4 + 0.6 * max(dot(normal, sun), 0.0));
           let glint = pow(max(dot(bounced, sun), 0.0), mix(1400.0, 220.0, clamp(distance / 1500.0, 0.0, 1.0))) * 22.0;
           let crest = smoothstep(0.80, 0.97, rise) * smoothstep(0.35, 0.75, fbm2(p.xz * 0.45 + frame.time * 0.2, 4));
           var color = mix(body, reflected, fresnel) + vec3f(1.0, 0.8, 0.55) * glint;
           color = mix(color, vec3f(0.85, 0.72, 0.66) * (0.6 + 0.4 * max(dot(normal, sun), 0.0)), crest * 0.55);
           color = mix(color, haze(ray), 1.0 - exp(-distance * 0.00045));
           return shown(color * 0.8, at);
       }
       """))
