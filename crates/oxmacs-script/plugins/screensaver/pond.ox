(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const EYE: f32 = 2.2;
       const BANK: f32 = 38.0;
       const CELL: f32 = 1.1;
       const PERIOD: f32 = 1.9;

       fn treeline(azimuth: f32) -> f32 {
           let crowns = fbm2(vec2f(azimuth * 9.0, 1.0), 5);
           let tall = 0.06 + 0.10 * crowns + 0.04 * pow(noise2(vec2f(azimuth * 40.0, 2.0)), 3.0);
           return tall;
       }

       fn sky(ray: vec3f) -> vec3f {
           let elevation = max(ray.y, 0.0);
           var color = mix(vec3f(0.42, 0.38, 0.40), vec3f(0.16, 0.20, 0.28), smoothstep(0.0, 0.5, elevation));
           let clouds = fbm2(ray.xz / max(ray.y, 0.05) * 1.5 + vec2f(frame.time * 0.02, 0.0), 6);
           color = mix(color, color * vec3f(0.7, 0.72, 0.8), smoothstep(0.4, 0.8, clouds) * 0.6);
           return color;
       }

       fn surroundings(ray: vec3f, lift: f32, scale: f32) -> vec3f {
           let azimuth = atan2(ray.x, ray.z);
           let elevation = ray.y;
           let top = lift + treeline(azimuth) * scale;
           if (elevation < top) {
               let mist = smoothstep(top, lift, elevation);
               let leaves = fbm2(vec2f(azimuth * 160.0, elevation * 400.0), 5);
               let trees = vec3f(0.014, 0.022, 0.016) * (0.4 + 1.2 * leaves);
               return mix(trees, vec3f(0.17, 0.17, 0.19), mist * 0.45);
           }
           return sky(ray);
       }

       fn ripples(p: vec2f) -> vec3f {
           let home = floor(p / CELL);
           var slope = vec2f(0.0);
           var splash = 0.0;
           for (var j = -1; j <= 1; j++) {
               for (var i = -1; i <= 1; i++) {
                   let cell = home + vec2f(f32(i), f32(j));
                   for (var k = 0; k < 2; k++) {
                       let seed = cell + vec2f(f32(k) * 57.0, 0.0);
                       let phase = cell2(seed + 3.0);
                       let cycle = floor(frame.time / PERIOD + phase);
                       let age = (frame.time / PERIOD + phase - cycle) * PERIOD;
                       let drop = (cell + cell22(seed + cycle * 13.0)) * CELL;
                       let offset = p - drop;
                       let d = length(offset);
                       let x = d - age * 0.42;
                       let strength = 0.010 * exp(-age * 1.6) / (1.0 + d * 4.0);
                       let packet = exp(-x * x * 220.0);
                       let dh = strength * packet * (55.0 * cos(x * 55.0) - 440.0 * x * sin(x * 55.0));
                       slope += dh * offset / max(d, 0.0001);
                       splash += exp(-d * d * 900.0) * smoothstep(0.12, 0.0, age);
                   }
               }
           }
           return vec3f(slope, splash);
       }

       fn pad(p: vec2f, center: vec2f, radius: f32, turn: f32) -> vec2f {
           let offset = rotation(turn + 0.1 * sin(frame.time * 0.4 + center.x)) * (p - center);
           let d = length(offset);
           let angle = atan2(offset.y, offset.x);
           let notch = smoothstep(0.10, 0.04, abs(angle)) * step(0.0, offset.x);
           let inside = smoothstep(radius, radius * 0.97, d) * (1.0 - notch);
           return vec2f(inside, d / radius);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let pitch = -0.40;
           let yaw = 0.04 * sin(frame.time * 0.02);
           let forward = normalize(vec3f(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch)));
           let side = normalize(cross(vec3f(0.0, 1.0, 0.0), forward));
           let up = cross(forward, side);
           let ray = normalize(forward + side * uv.x + up * uv.y);
           var color: vec3f;
           let t = -EYE / min(ray.y, -0.0001);
           let p = vec3f(0.0, EYE, 0.0) + ray * t;
           if (ray.y >= 0.0 || p.z > BANK) {
               color = surroundings(ray, -EYE / BANK * ray.z, 1.0);
           } else {
               let wave = ripples(p.xz);
               let fade = smoothstep(BANK * 0.7, 6.0, t);
               let normal = normalize(vec3f(-wave.x * fade, 1.0, -wave.y * fade));
               var bounced = reflect(ray, normal);
               bounced.y = max(bounced.y, 0.002);
               let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(normal, -ray), 0.0), 5.0);
               let body = vec3f(0.012, 0.018, 0.014) * (0.8 + 0.4 * fbm2(p.xz * 0.6, 3));
               let closer = BANK / max(BANK - p.z, 1.0);
               color = mix(body, surroundings(bounced, 0.0, closer), fresnel);
               color += vec3f(0.5, 0.52, 0.55) * wave.z * 0.6;
               let pads = array<vec4f, 6>(
                   vec4f(-1.6, 6.5, 0.55, 0.4),
                   vec4f(-0.9, 7.6, 0.42, 2.1),
                   vec4f(1.8, 9.5, 0.60, 4.0),
                   vec4f(2.6, 8.4, 0.38, 1.0),
                   vec4f(-3.4, 12.0, 0.70, 5.2),
                   vec4f(0.6, 15.0, 0.50, 3.3));
               for (var i = 0; i < 6; i++) {
                   let leaf = pads[i];
                   let found = pad(p.xz, leaf.xy, leaf.z, leaf.w);
                   if (found.x > 0.0) {
                       let veins = smoothstep(0.94, 1.0, abs(sin(atan2(p.z - leaf.y, p.x - leaf.x) * 9.0))) * 0.2;
                       let green = mix(vec3f(0.018, 0.040, 0.014), vec3f(0.045, 0.075, 0.025), fbm2(p.xz * 4.0 + leaf.xy, 4));
                       let rim = smoothstep(0.85, 1.0, found.y) * 0.35;
                       let sheen = surroundings(reflect(ray, vec3f(0.0, 1.0, 0.0)), 0.0, closer) * 0.12;
                       color = mix(color, green * (1.0 - veins) * (1.0 - rim) + sheen, found.x);
                   }
               }
               color = mix(color, vec3f(0.24, 0.24, 0.26), smoothstep(10.0, BANK, t) * 0.35);
           }
           var streaks = 0.0;
           for (var layer = 0; layer < 3; layer++) {
               let scale = vec2f(45.0, 2.2) * (1.0 + f32(layer) * 0.7);
               let moving = vec2f(uv.x + uv.y * 0.12, uv.y) * scale + vec2f(0.0, frame.time * scale.y * 3.2);
               let cell = floor(moving);
               let local = fract(moving);
               let jitter = cell2(cell + f32(layer) * 17.0);
               let reach = 0.15 + 0.25 * cell2(cell + 5.0);
               streaks += smoothstep(0.035, 0.0, abs(local.x - jitter)) * smoothstep(0.0, 0.1, local.y) * smoothstep(reach, 0.0, local.y) * step(0.82, cell2(cell + 9.0)) / (1.0 + f32(layer));
           }
           color += vec3f(0.30, 0.31, 0.34) * streaks * 0.12;
           color *= 1.0 - smoothstep(0.55, 1.2, length(uv)) * 0.5;
           return shown(color * 1.5, at);
       }
       """))
