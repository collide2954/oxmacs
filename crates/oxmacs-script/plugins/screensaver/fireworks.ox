(module
  (import (ox math) (screensaver noise :as noise) (screensaver shading :as shading))
  (export shader rocket place draw))

(def palette ["#ff4d6d" "#ffd166" "#3ddc97" "#4cc9f0" "#f72585" "#fff3d6" "#ff9f1c" "#c77dff"])
(def period 0.9)
(def rise 1.5)
(def burn 3.2)
(def gravity 60.0)

(def shader
  (str shading/library
       """
       const COLORS = array<vec3f, 8>(
           vec3f(1.0, 0.30, 0.43), vec3f(1.0, 0.82, 0.40), vec3f(0.24, 0.86, 0.59), vec3f(0.30, 0.79, 0.94),
           vec3f(0.97, 0.15, 0.52), vec3f(1.0, 0.95, 0.84), vec3f(1.0, 0.62, 0.11), vec3f(0.78, 0.49, 1.0));

       fn bursts(p: vec2f) -> vec3f {
           var light = vec3f(0.0);
           for (var i = 0; i < 4; i++) {
               let at = vec2f(input(i * 4), input(i * 4 + 1)) * frame.size;
               let bright = input(i * 4 + 2);
               let tint = COLORS[i32(input(i * 4 + 3)) % 8];
               let d = length(p - at) / frame.size.y;
               light += tint * bright * (exp(-d * 5.0) * 0.10 + exp(-d * 22.0) * 0.12);
           }
           return light;
       }

       fn skyline(x: f32) -> f32 {
           let cell = floor(x * 38.0);
           let local = fract(x * 38.0);
           let tall = 0.03 + 0.09 * pow(cell2(vec2f(cell, 1.0)), 2.0) + 0.01 * noise2(vec2f(x * 3.0, 2.0));
           let narrow = step(0.25 * cell2(vec2f(cell, 4.0)), local) * step(local, 1.0 - 0.25 * cell2(vec2f(cell, 5.0)));
           let spire = step(abs(local - 0.5), 0.04) * step(0.85, cell2(vec2f(cell, 6.0))) * 0.03;
           return 0.80 - max(tall * narrow, 0.02) - spire;
       }

       fn sky(p: vec2f) -> vec3f {
           let y = p.y / frame.size.y;
           var color = mix(vec3f(0.010, 0.014, 0.035), vec3f(0.05, 0.035, 0.06), smoothstep(0.2, 0.85, y));
           let grid = p / frame.size.y * 220.0;
           let star = pow(cell2(floor(grid)), 70.0) * exp(-dot(fract(grid) - 0.5, fract(grid) - 0.5) * 40.0);
           color += vec3f(0.8, 0.85, 1.0) * star * smoothstep(0.6, 0.2, y);
           let drift = vec2f(frame.time * 0.01, 0.0);
           let smoke = smoothstep(0.45, 0.85, fbm2(p / frame.size.y * 3.0 + drift, 5)) * smoothstep(0.05, 0.4, y) * smoothstep(0.85, 0.5, y);
           let light = bursts(p);
           color += light * (0.5 + 1.6 * smoke);
           color += vec3f(0.04, 0.035, 0.04) * smoke;
           return color;
       }

       fn shade(at: vec2f) -> vec4f {
           let x = at.x / frame.size.x;
           let y = at.y / frame.size.y;
           var color = sky(at);
           let roof = skyline(x);
           let shore = 0.84;
           if (y > roof && y < shore) {
               let wx = fract(x * 300.0);
               let wy = fract(y * 220.0);
               let lit = step(0.55, wx) * step(0.45, wy) * step(0.9, cell2(floor(vec2f(x * 300.0, y * 220.0) / vec2f(1.0, 1.0))));
               color = vec3f(0.006, 0.007, 0.012) + vec3f(1.0, 0.75, 0.4) * lit * 0.5 + bursts(at) * 0.04;
           }
           if (y >= shore) {
               let below = y - shore;
               let ripple = (fbm2(vec2f(x * 40.0, below * 300.0 / (below + 0.02) - frame.time * 0.6), 3) - 0.5) * 0.02;
               let mirror = vec2f(at.x + ripple * frame.size.x, (shore - below * 1.3) * frame.size.y);
               var reflected = sky(mirror) * 0.5;
               let mx = mirror.x / frame.size.x;
               if (mirror.y / frame.size.y > skyline(mx)) {
                   reflected = vec3f(0.004);
               }
               color = reflected + bursts(vec2f(mirror.x, mirror.y)) * 0.3 * smoothstep(0.0, 0.08, below);
           }
           return shown(color, at);
       }
       """))

(def (rocket i width height)
  "Rocket i: when it goes up, where it bursts and how."
  (let [pick (noise/of [i :kind])
        kind (cond [(< pick 0.5) 0] [(< pick 0.8) 1] [else 2])]
    {id: i
     kind: kind
     launch: (- (* period (+ i (* 0.6 (noise/of [i :when])))) rise)
     x: (* width (+ 0.1 (* 0.8 (noise/of [i :x]))))
     ground: (* 0.8 height)
     peak: (* height (+ 0.10 (* 0.30 (noise/of [i :peak]))))
     hue: (int (* (count palette) (noise/of [i :color])))
     sparks: (+ 70 (int (* 40 (noise/of [i :sparks]))))
     speed: (* height (if (= kind 1) 0.2 (+ 0.24 (* 0.12 (noise/of [i :speed])))))}))

(def (place r angle pace dt)
  (let [drag (if (= r.kind 1) 2.6 1.5)
        drift (/ (- 1 (math/exp (* (- drag) dt))) drag)
        fall (if (= r.kind 1) 1.7 1.0)]
    [(+ r.x (* pace (math/cos angle) drift))
     (+ r.peak (* pace (math/sin angle) drift) (* 0.5 gravity fall dt dt))]))

(def (spark r k dt fade)
  (let [ring (= r.kind 2)
        angle (+ (* 2 math/pi (/ k r.sparks)) (* (if ring 0.02 0.6) (noise/of [r.id k :angle])))
        depth (- (* 2 (noise/of [r.id k :depth])) 1)
        pace (* r.speed (if ring 0.9 (* (+ 0.92 (* 0.08 (noise/of [r.id k :pace]))) (math/sqrt (- 1 (* depth depth))))))
        tail (cond [(= r.kind 1) 0.7] [ring 0.25] [else 0.5])
        trail (vec (map (fn [s] (place r angle pace (max 0 (- dt (* tail (/ s 5)))))) [5 4 3 2 1 0]))
        [x y] (last trail)
        color (nth palette (if (= r.kind 1) 6 r.hue))
        twinkle (if (> dt 1.6) (+ 0.5 (* 0.5 (math/sin (+ (* 41 dt) (* 7 k))))) 1.0)
        bright (* fade twinkle)]
    [[:curve trail [color (* 0.55 bright)] 1.3 1.0]
     [:glow x y 7 [color (* 0.75 bright)]]
     [:circle x y 1.3 ["#fff8ee" (* 0.95 bright)]]]))

(def (shell r seconds)
  (let [age (- seconds r.launch)]
    (cond
      [(< age 0) {shapes: [] light: nil}]
      [(< age rise)
       (let [height-at (fn [a] (- r.ground (* (- r.ground r.peak) (- 1 (math/pow (- 1 (/ a rise)) 2)))))
             wobble (fn [a] (+ r.x (* 3 (math/sin (* 30 a)))))
             trail (vec (map (fn [s] (let [a (max 0 (- age (* 0.06 s)))] [(wobble a) (height-at a)])) [5 4 3 2 1 0]))
             [x y] (last trail)]
         {shapes: [[:curve trail ["#ffd9a0" 0.6] 1.4 1.0]
                   [:glow x y 9 ["#ffcc88" 0.8]]
                   [:circle x y 1.3 "#fff4e0"]]
          light: nil})]
      [(< age (+ rise burn))
       (let [dt (- age rise)
             fade (math/pow (- 1 (/ dt burn)) 1.4)
             flash (max 0 (- 1 (/ dt 0.25)))
             core (if (> flash 0) [[:glow r.x r.peak (* 60 flash) [(nth palette r.hue) (* 0.5 flash)]]] [])]
         {shapes: (vec (concat core (mapcat (fn [k] (spark r k dt fade)) (range r.sparks))))
          light: [r.x r.peak (+ (* 2.5 flash) (* 0.6 fade)) r.hue]})]
      [else {shapes: [] light: nil}])))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        oldest (max 0 (- (int (/ (- seconds burn) period)) 1))
        shells (map (fn [i] (shell (rocket i frame.width frame.height) seconds))
                    (range oldest (+ 2 (int (/ (+ seconds rise) period)))))
        lights (take 4 (reverse (keep (fn [s] s.light) shells)))]
    {shapes: (vec (mapcat (fn [s] s.shapes) shells))
     inputs: (vec (mapcat (fn [[x y bright hue]] [(/ x frame.width) (/ y frame.height) bright hue]) lights))}))

(test "rockets burst high above the skyline"
  (expect (every? (fn [i]
                    (let [r (rocket i 800 600)]
                      (< r.peak (* 0.5 r.ground))))
                  (range 50))))

(test "sparks fall once they slow down"
  (let [r (rocket 3 800 600)
        [_ early] (place r 0 100 0.5)
        [_ late] (place r 0 100 3.0)]
    (expect (> late early))))
