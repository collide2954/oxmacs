(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       fn ridge(azimuth: f32) -> f32 {
           let far = 0.035 + 0.05 * fbm2(vec2f(azimuth * 2.2, 3.0), 5);
           let near = 0.012 + 0.07 * pow(fbm2(vec2f(azimuth * 4.5 + 9.0, 1.0), 5), 1.6);
           return max(far, near - 0.02);
       }

       fn trees(azimuth: f32, elevation: f32) -> f32 {
           let x = azimuth * 140.0;
           let cell = floor(x);
           let local = fract(x) - 0.5;
           let tall = 0.010 + 0.022 * cell2(vec2f(cell, 4.0));
           let crown = tall * (1.0 - abs(local) * 2.2) - 0.003 * sin(x * 9.0);
           return step(elevation, 0.004 + max(crown, 0.0) * step(0.35, cell2(vec2f(cell, 7.0))));
       }

       fn stars(ray: vec3f) -> vec3f {
           let azimuth = atan2(ray.x, ray.z);
           let elevation = asin(clamp(ray.y, -1.0, 1.0));
           let grid = vec2f(azimuth, elevation) * 260.0;
           let cell = floor(grid);
           let spot = cell22(cell);
           let offset = fract(grid) - spot;
           let bright = pow(cell2(cell + 31.0), 22.0);
           let twinkle = 0.7 + 0.3 * sin(frame.time * (1.0 + 3.0 * spot.x) + spot.y * 40.0);
           let tint = mix(vec3f(0.7, 0.8, 1.0), vec3f(1.0, 0.85, 0.7), spot.y);
           let band = exp(-pow((elevation - 0.55 - 0.25 * sin(azimuth)) * 3.0, 2.0));
           let haze = vec3f(0.020, 0.022, 0.032) * band * smoothstep(0.3, 0.8, fbm2(vec2f(azimuth, elevation) * 5.0, 6));
           return tint * bright * twinkle * exp(-dot(offset, offset) * 900.0) * 6.0 + haze;
       }

       fn lights(origin: vec3f, ray: vec3f) -> vec3f {
           if (ray.y <= 0.0) {
               return vec3f(0.0);
           }
           var glow = vec3f(0.0);
           let steps = 72;
           let jitter = cell2(floor(ray.xy * 4000.0));
           for (var i = 0; i < steps; i++) {
               let along = (f32(i) + jitter) / f32(steps);
               let height = 1.0 + along * 2.4;
               let t = (height - origin.y) / ray.y;
               if (t > 60.0) {
                   break;
               }
               let p = origin.xz + ray.xz * t;
               let drift = frame.time * 0.035;
               let warp = vec2f(fbm2(p * 0.09 + vec2f(drift, 0.0), 4), fbm2(p * 0.09 + vec2f(3.1, drift), 4));
               let q = p + (warp - 0.5) * 9.0;
               let line = q.y - 10.0 - 3.0 * sin(q.x * 0.10 + frame.time * 0.05) - 1.5 * sin(q.x * 0.27 - frame.time * 0.08);
               let fold = exp(-line * line * 7.0) + 0.6 * exp(-pow(line + 5.0, 2.0) * 5.0) + 0.4 * exp(-pow(line - 7.0, 2.0) * 3.0);
               let rays = 0.15 + 0.85 * pow(noise2(vec2f(q.x * 3.2 + line * 0.3, frame.time * 0.35)), 3.0) + 0.3 * pow(noise2(vec2f(q.x * 9.0, frame.time * 0.6)), 4.0);
               let lift = along;
               let edge = smoothstep(0.0, 0.025, lift) * exp(-lift * 3.2);
               let color = mix(vec3f(0.08, 1.0, 0.32), vec3f(0.65, 0.12, 0.70), smoothstep(0.30, 0.95, lift));
               glow += color * fold * rays * edge * (0.8 + 0.5 * sin(frame.time * 0.4 + q.x * 0.05));
           }
           return glow * 0.22;
       }

       fn sky(origin: vec3f, ray: vec3f) -> vec3f {
           let elevation = asin(clamp(ray.y, -1.0, 1.0));
           var color = mix(vec3f(0.010, 0.022, 0.040), vec3f(0.002, 0.004, 0.012), smoothstep(0.0, 0.6, elevation));
           color += stars(ray);
           color += lights(origin, ray);
           return color;
       }

       fn above(origin: vec3f, ray: vec3f, glow: vec3f) -> vec3f {
           let azimuth = atan2(ray.x, ray.z);
           let elevation = asin(clamp(ray.y, -1.0, 1.0));
           if (trees(azimuth, elevation) > 0.5) {
               return vec3f(0.002, 0.003, 0.005);
           }
           let mountain = ridge(azimuth);
           if (elevation < mountain) {
               let rock = fbm2(vec2f(azimuth * 140.0, elevation * 500.0), 5);
               let snow = smoothstep(0.5, 0.7, rock) * smoothstep(0.2 * mountain, mountain, elevation);
               let rim = smoothstep(mountain - 0.006, mountain, elevation);
               return vec3f(0.003, 0.004, 0.007) + glow * (0.015 + 0.05 * snow + 0.08 * rim) + vec3f(0.004, 0.005, 0.008) * snow;
           }
           return sky(origin, ray);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let yaw = 0.15 * sin(frame.time * 0.01);
           let forward = normalize(vec3f(sin(yaw), 0.17, cos(yaw)));
           let side = normalize(cross(vec3f(0.0, 1.0, 0.0), forward));
           let up = cross(forward, side);
           let origin = vec3f(0.0, 0.0, 0.0);
           let ray = normalize(forward + side * uv.x * 1.1 + up * uv.y * 1.1);
           let glow = lights(origin, normalize(vec3f(forward.x, 0.35, forward.z))) * 2.0;
           if (ray.y >= 0.0) {
               return shown(above(origin, ray, glow), at);
           }
           let depth = -ray.y;
           let ripple = vec2f(
               fbm2(vec2f(ray.x / depth * 3.0, 1.0 / depth * 8.0 + frame.time * 0.3), 3),
               fbm2(vec2f(ray.x / depth * 3.0 + 5.0, 1.0 / depth * 8.0 - frame.time * 0.2), 3)) - 0.5;
           let mirrored = normalize(vec3f(ray.x + ripple.x * 0.012, depth + ripple.y * 0.006, ray.z));
           let fresnel = 0.02 + 0.98 * pow(1.0 - depth, 5.0);
           let reflected = above(origin, mirrored, glow) * mix(0.5, 0.85, fresnel);
           return shown(reflected, at);
       }
       """))
