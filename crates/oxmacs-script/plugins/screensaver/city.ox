(module
  (import (screensaver shading :as shading))
  (export shader))

(def shader
  (str shading/library
       """
       const SHORE: f32 = -0.12;

       fn sky(uv: vec2f) -> vec3f {
           let height = uv.y - SHORE;
           var color = mix(vec3f(0.30, 0.16, 0.12), vec3f(0.010, 0.016, 0.040), pow(smoothstep(0.0, 0.6, height), 0.6));
           let clouds = fbm2(vec2f(uv.x * 1.5 + frame.time * 0.004, uv.y * 4.0), 6);
           color += vec3f(0.22, 0.10, 0.07) * smoothstep(0.45, 0.75, clouds) * exp(-height * 3.0) * 0.6;
           return color;
       }

       struct Building {
           cover: f32,
           color: vec3f,
       }

       fn skyline(uv: vec2f, layer: f32) -> Building {
           let scale = 9.0 + layer * 7.0;
           let x = uv.x * scale + layer * 31.0;
           let cell = floor(x);
           let local = fract(x);
           let tall = (0.10 + 0.32 * pow(cell2(vec2f(cell, layer)), 1.6)) * (1.25 - layer * 0.28);
           let setback = step(0.6, cell2(vec2f(cell, layer + 6.0)));
           let upper = step(SHORE + tall * 0.78, uv.y) * setback;
           let inset = 0.06 + 0.12 * cell2(vec2f(cell, layer + 4.0)) + upper * 0.12;
           let top = SHORE + tall;
           var cover = step(inset, local) * step(local, 1.0 - inset) * step(uv.y, top) * step(SHORE - 0.01, uv.y);
           let spire = step(abs(local - 0.5), 0.015) * step(uv.y, top + 0.04 * step(0.82, cell2(vec2f(cell, layer + 8.0)))) * step(top, uv.y);
           cover = max(cover, spire);
           var color = vec3f(0.010, 0.012, 0.018) + vec3f(0.035, 0.035, 0.05) * layer * 0.6;
           let span = 1.0 - 2.0 * inset;
           let across = (local - inset) / span;
           let columns = 6.0 + floor(cell2(vec2f(cell, layer + 2.0)) * 8.0);
           let floors = tall * 150.0 / (1.0 + layer * 0.5);
           let wx = across * columns;
           let wy = (uv.y - SHORE) / tall * floors;
           let ribbon = step(0.7, cell2(vec2f(cell, layer + 14.0)));
           let window = mix(step(0.25, fract(wx)) * step(fract(wx), 0.8), step(0.05, fract(wx)), ribbon) * step(0.3, fract(wy)) * step(fract(wy), 0.85);
           let epoch = floor(frame.time / 7.0 + cell2(vec2f(floor(wx), floor(wy)) + cell));
           let busy = 0.55 - 0.2 * cell2(vec2f(cell, floor(wy))) + ribbon * 0.25;
           let on = step(busy, cell2(vec2f(floor(wx * (1.0 - ribbon)) + cell * 17.0, floor(wy) + epoch * 3.0)));
           let warm = mix(vec3f(1.0, 0.72, 0.38), vec3f(0.75, 0.85, 1.0), step(0.75, cell2(vec2f(cell, floor(wy) + 40.0))));
           color += warm * window * on * step(uv.y, top - 0.005) * (0.9 / (1.0 + layer * 0.8)) * (1.0 - spire);
           let beacon = step(0.6, cell2(vec2f(cell, layer + 12.0))) * pow(max(sin(frame.time * 2.0 + cell), 0.0), 8.0);
           let tip = length(vec2f((local - 0.5) / scale, uv.y - top - 0.004));
           color += vec3f(1.0, 0.08, 0.04) * beacon * exp(-tip * 900.0) * 3.0;
           return Building(cover, color);
       }

       fn scene(uv: vec2f) -> vec3f {
           var color = sky(uv);
           for (var layer = 2; layer >= 0; layer--) {
               let found = skyline(uv, f32(layer));
               let haze = vec3f(0.12, 0.07, 0.07) * f32(layer) * 0.30;
               color = mix(color, found.color * (1.0 - f32(layer) * 0.2) + haze, found.cover);
           }
           return color;
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0) + vec2f(frame.time * 0.004, 0.0);
           var color: vec3f;
           if (uv.y >= SHORE) {
               color = scene(uv);
           } else {
               let below = SHORE - uv.y;
               let ripple = (fbm2(vec2f(uv.x * 4.0, below * 90.0 / (below + 0.05) - frame.time * 0.5), 4) - 0.5) * 0.03 * (0.3 + below * 4.0);
               let waves = (fbm2(vec2f(uv.x * 2.0, below * 260.0 / (below + 0.03) - frame.time * 0.8), 3) - 0.5) * 0.05 * (0.2 + below * 5.0);
               let mirrored = vec2f(uv.x + ripple + waves, SHORE + below * (1.0 + 0.6 * below));
               var reflected = vec3f(0.0);
               for (var k = 0; k < 6; k++) {
                   reflected += scene(mirrored + vec2f(0.0, f32(k) * 0.006 * (0.3 + below * 4.0))) / 6.0;
               }
               let streak = pow(max(reflected - vec3f(0.08), vec3f(0.0)), vec3f(1.2)) * 1.4;
               color = reflected * 0.22 * vec3f(0.85, 0.9, 1.0) + streak * 0.5 + vec3f(0.004, 0.007, 0.014);
               let road = abs(uv.y - (SHORE - 0.012));
               color = mix(color, vec3f(0.01), smoothstep(0.004, 0.002, road));
               for (var i = 0; i < 6; i++) {
                   let s = f32(i);
                   let way = select(-1.0, 1.0, i % 2 == 0);
                   let x = fract(frame.time * 0.03 * (1.0 + s * 0.15) * way + s * 0.37) * 2.4 - 1.2 + frame.time * 0.004;
                   let car = length(vec2f((uv.x - x) * 0.6, road));
                   let lamp = select(vec3f(1.0, 0.9, 0.7), vec3f(1.0, 0.1, 0.05), way < 0.0);
                   color += lamp * exp(-car * 700.0) * 2.0;
               }
           }
           color *= 1.0 - smoothstep(0.6, 1.3, length((at - frame.size * 0.5) / frame.size.y)) * 0.5;
           return shown(color * 1.2, at);
       }
       """))
