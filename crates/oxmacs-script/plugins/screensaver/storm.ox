(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const PERIOD: f32 = 5.5;
       const BASE: f32 = 0.30;
       const LAND: f32 = -0.16;

       struct Strike {
           seed: f32,
           x: f32,
           age: f32,
           power: f32,
       }

       fn strike() -> Strike {
           let cycle = floor(frame.time / PERIOD);
           let into = frame.time - cycle * PERIOD;
           let at = 0.5 + 2.8 * cell2(vec2f(cycle, 1.0));
           let age = into - at;
           let x = (cell2(vec2f(cycle, 2.0)) - 0.5) * 1.3;
           var power = 0.0;
           if (age >= 0.0) {
               let pulses = step(age, 0.12) + 0.7 * step(0.20, age) * step(age, 0.30) + 0.9 * step(0.42, age) * step(age, 0.56);
               power = max(pulses, exp(-age * 3.0) * 0.5) * step(0.2, cell2(vec2f(cycle, 3.0)));
           }
           return Strike(cycle, x, age, power);
       }

       fn path(y: f32, seed: f32) -> f32 {
           var x = 0.0;
           var amplitude = 0.10;
           var frequency = 3.0;
           for (var i = 0; i < 6; i++) {
               x += (noise2(vec2f(y * frequency, seed * 13.0 + f32(i) * 7.0)) - 0.5) * amplitude;
               amplitude *= 0.55;
               frequency *= 2.2;
           }
           return x;
       }

       fn bolt(p: vec2f, s: Strike) -> f32 {
           if (s.power <= 0.0) {
               return 0.0;
           }
           let down = (BASE - p.y) / (BASE - LAND);
           var light = 0.0;
           if (down > 0.0 && down < 1.0) {
               let d = abs(p.x - s.x - path(down, s.seed));
               light += 0.004 / (d + 0.004) * 0.5 + exp(-d * 700.0) * 6.0;
           }
           for (var b = 0; b < 4; b++) {
               let fork = 0.15 + 0.55 * cell2(vec2f(s.seed, 10.0 + f32(b)));
               let span = 0.12 + 0.25 * cell2(vec2f(s.seed, 20.0 + f32(b)));
               let lean = (cell2(vec2f(s.seed, 30.0 + f32(b))) - 0.5) * 0.9;
               let along = (down - fork) / span;
               if (along > 0.0 && along < 1.0) {
                   let x = s.x + path(fork, s.seed) + lean * (down - fork) + path(down * 2.0, s.seed + f32(b) + 1.0) * 0.5;
                   let d = abs(p.x - x);
                   let fade = 1.0 - along;
                   light += (0.002 / (d + 0.002) * 0.3 + exp(-d * 1200.0) * 3.0) * fade;
               }
           }
           return light * s.power * smoothstep(0.0, 0.02, down);
       }

       fn clouds(p: vec2f, s: Strike) -> vec3f {
           let height = max(p.y - LAND, 0.004);
           let plane = vec2f(p.x / height, 1.0 / height) * 0.35 + vec2f(frame.time * 0.03, frame.time * 0.08);
           let warp = vec2f(fbm2(plane * 0.6 + frame.time * 0.02, 4), fbm2(plane * 0.6 + 7.0, 4));
           let billow = fbm2(plane * 1.2 + warp * 1.6, 7);
           let thick = smoothstep(0.30, 0.70, billow);
           let fade = smoothstep(0.0, 0.22, height);
           let haze = vec3f(0.030, 0.033, 0.045);
           let lit = mix(vec3f(0.050, 0.055, 0.070), vec3f(0.012, 0.014, 0.020), thick);
           let base = mix(haze, lit, fade);
           let near = exp(-abs(p.x - s.x) * 2.2) * exp(-abs(p.y - BASE) * 2.5);
           let flash = s.power * (0.25 + 1.8 * near) * (0.35 + 0.9 * (1.0 - thick) + 0.6 * billow);
           return base + vec3f(0.55, 0.55, 0.78) * flash * 0.45;
       }

       fn hills(x: f32) -> f32 {
           return LAND + 0.03 + 0.05 * fbm2(vec2f(x * 1.4, 2.0), 5) + 0.025 * sin(x * 1.3);
       }

       fn far_hills(x: f32) -> f32 {
           return LAND + 0.06 + 0.06 * fbm2(vec2f(x * 2.6, 9.0), 5);
       }

       fn rain(p: vec2f, s: Strike) -> f32 {
           var sum = 0.0;
           for (var layer = 0; layer < 4; layer++) {
               let depth = 1.0 + f32(layer) * 0.8;
               let slant = vec2f(p.x + p.y * 0.18, p.y);
               let scale = vec2f(90.0, 4.0) * depth;
               let moving = slant * scale + vec2f(0.0, frame.time * scale.y * 2.4 / depth);
               let cell = floor(moving);
               let local = fract(moving);
               let jitter = cell2(cell + f32(layer) * 31.0);
               let column = abs(local.x - 0.5 - (jitter - 0.5) * 0.6);
               let drop = smoothstep(0.08, 0.0, column) * smoothstep(0.0, 0.3, local.y) * smoothstep(1.0, 0.5, local.y) * step(0.55, jitter);
               sum += drop * (0.5 / depth);
           }
           return sum;
       }

       fn shade(at: vec2f) -> vec4f {
           let p = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let s = strike();
           let ground = hills(p.x);
           var color = clouds(p, s);
           let veil = smoothstep(0.3, 0.8, fbm2(vec2f(p.x * 3.0 + p.y * 0.6 - frame.time * 0.06, 1.0), 4)) * smoothstep(BASE + 0.1, LAND, p.y);
           color = mix(color, vec3f(0.040, 0.043, 0.056) * (1.0 + s.power * 3.0), veil * 0.55);
           color += vec3f(0.75, 0.70, 1.0) * bolt(p, s);
           let distant = far_hills(p.x);
           if (p.y < distant) {
               let dark = vec3f(0.016, 0.018, 0.024) + vec3f(0.10, 0.10, 0.14) * s.power * exp(-abs(p.x - s.x) * 1.2) * 0.5;
               color = mix(color, dark, 0.85);
           }
           let horizon_glow = exp(-abs(p.y - distant) * 14.0) * s.power * exp(-abs(p.x - s.x) * 1.5);
           color += vec3f(0.45, 0.45, 0.65) * horizon_glow * 0.35;
           if (p.y < ground) {
               let below = ground - p.y;
               let depth = 1.0 / max(below, 0.01);
               let grass = fbm2(vec2f(p.x * depth * 3.0, depth * 2.0), 5);
               let blades = noise2(vec2f(p.x * 900.0, p.y * 90.0));
               let puddle = smoothstep(0.6, 0.72, fbm2(vec2f(p.x * depth * 0.8, depth * 0.6) + 4.0, 4)) * smoothstep(0.05, 0.25, below) * 0.6;
               let wet = vec3f(0.030, 0.034, 0.045) * (1.0 + s.power * 4.0) * exp(-below * 4.0);
               color = vec3f(0.006, 0.009, 0.007) * (0.5 + 0.9 * grass) * (0.7 + 0.6 * blades) + wet * (0.25 + 0.75 * puddle) + vec3f(0.12, 0.13, 0.17) * s.power * (0.3 + 0.7 * grass) * exp(-below * 5.0) * 0.35;
           }
           let trunk = abs(p.x - 0.42);
           let crown = length((p - vec2f(0.42, hills(0.42) + 0.075)) * vec2f(1.0, 1.4));
           let leaves = smoothstep(0.065, 0.055, crown + (fbm2(p * 60.0, 4) - 0.5) * 0.03);
           let stem = step(trunk, 0.004) * step(p.y, hills(0.42) + 0.06) * step(hills(0.42) - 0.01, p.y);
           color = mix(color, vec3f(0.002, 0.003, 0.003), max(leaves, stem));
           color += vec3f(0.55, 0.58, 0.68) * rain(p, s) * (0.10 + 0.9 * s.power);
           color *= 1.0 - smoothstep(0.5, 1.2, length(p * vec2f(0.7, 1.0))) * 0.6;
           return shown(color * 1.2, at);
       }
       """))
