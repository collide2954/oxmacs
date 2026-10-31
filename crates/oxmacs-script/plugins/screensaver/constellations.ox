(module
  (import (ox math) (screensaver shading :as shading))
  (export shader figures project draw))

(def shader
  (str shading/library
       """
       fn starfield(p: vec2f, scale: f32, rarity: f32) -> vec3f {
           let grid = p * scale;
           let cell = floor(grid);
           let spot = 0.2 + 0.6 * cell22(cell);
           let offset = fract(grid) - spot;
           let bright = pow(cell2(cell + 17.0), rarity);
           let tint = mix(vec3f(0.65, 0.75, 1.0), vec3f(1.0, 0.80, 0.6), cell2(cell + 5.0));
           let twinkle = 0.8 + 0.2 * sin(frame.time * (1.0 + 2.0 * spot.x) + spot.y * 30.0);
           return tint * bright * twinkle * exp(-dot(offset, offset) * scale * 3.0);
       }

       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y * vec2f(1.0, -1.0);
           let turn = rotation(frame.time * 0.004);
           let sky = turn * uv;
           let band = sky.y + 0.35 * sky.x - 0.05;
           let milky = exp(-band * band * 12.0);
           let clouds = smoothstep(0.35, 0.85, fbm2(sky * 11.0 + 2.0, 6)) * (0.5 + 0.5 * fbm2(sky * 3.0, 4));
           let lanes = smoothstep(0.5, 0.72, fbm2(sky * 9.0 + 9.0, 5)) * exp(-band * band * 40.0);
           var color = mix(vec3f(0.006, 0.009, 0.020), vec3f(0.002, 0.003, 0.008), smoothstep(-0.5, 0.5, uv.y));
           color += vec3f(0.020, 0.019, 0.024) * milky * (0.25 + clouds) * (1.0 - lanes * 0.9);
           color += vec3f(0.035, 0.026, 0.020) * exp(-length(sky - vec2f(0.25, -0.12)) * 3.5) * milky;
           color += starfield(sky, 260.0, 30.0) * 2.0;
           color += starfield(sky + 7.0, 520.0, 18.0) * (0.5 + milky * 2.5 * (1.0 - lanes));
           color += starfield(sky + 3.0, 900.0, 10.0) * milky * 1.6 * (1.0 - lanes);
           color += starfield(sky + 5.0, 1400.0, 8.0) * milky * 0.9 * (1.0 - lanes);
           color += vec3f(0.02, 0.05, 0.03) * exp(-(uv.y + 0.5) * 9.0);
           let ridge = -0.40 + 0.03 * fbm2(vec2f(uv.x * 3.0, 1.0), 4) + 0.06 * pow(noise2(vec2f(uv.x * 18.0, 2.0)), 3.0);
           if (uv.y < ridge) {
               color = vec3f(0.002, 0.003, 0.004);
           }
           return shown(color * 1.2, at);
       }
       """))

(def figures
  [{name: "Orion"
    stars: [[5.92 7.41 0.5 1.0] [5.42 6.35 1.6 0.15] [5.53 -0.30 2.2 0.1] [5.60 -1.20 1.7 0.1]
            [5.68 -1.94 1.8 0.1] [5.80 -9.67 2.1 0.1] [5.24 -8.20 0.1 0.05] [5.59 9.93 3.4 0.15]]
    lines: [[7 0] [7 1] [0 4] [1 2] [2 3] [3 4] [4 5] [2 6]]}
   {name: "Ursa Major"
    stars: [[11.06 61.75 1.8 0.8] [11.03 56.38 2.4 0.3] [11.90 53.69 2.4 0.3] [12.26 57.03 3.3 0.3]
            [12.90 55.96 1.8 0.3] [13.40 54.93 2.2 0.3] [13.79 49.31 1.9 0.2]]
    lines: [[0 1] [1 2] [2 3] [3 0] [3 4] [4 5] [5 6]]}
   {name: "Cassiopeia"
    stars: [[0.15 59.15 2.3 0.4] [0.68 56.54 2.2 0.8] [0.95 60.72 2.2 0.2] [1.43 60.24 2.7 0.3] [1.91 63.67 3.4 0.3]]
    lines: [[0 1] [1 2] [2 3] [3 4]]}
   {name: "Cygnus"
    stars: [[20.69 45.28 1.3 0.3] [20.37 40.26 2.2 0.5] [19.51 27.96 3.1 0.8] [20.77 33.97 2.5 0.7] [19.75 45.13 2.9 0.25]]
    lines: [[0 1] [1 2] [1 3] [1 4]]}
   {name: "Leo"
    stars: [[10.14 11.97 1.4 0.2] [10.12 16.76 3.5 0.3] [10.33 19.84 2.0 0.8] [10.28 23.42 3.4 0.3] [9.88 26.01 3.9 0.6]
            [9.76 23.77 3.0 0.6] [11.24 20.52 2.6 0.3] [11.24 15.43 3.3 0.3] [11.82 14.57 2.1 0.3]]
    lines: [[0 1] [1 2] [2 3] [3 4] [4 5] [2 6] [6 8] [8 7] [7 0]]}
   {name: "Scorpius"
    stars: [[16.49 -26.43 1.0 1.0] [16.01 -22.62 2.3 0.15] [16.09 -19.81 2.6 0.15] [15.98 -26.11 2.9 0.15]
            [16.35 -25.59 2.9 0.15] [16.60 -28.22 2.8 0.15] [16.84 -34.29 2.3 0.7] [16.86 -38.05 3.0 0.15]
            [16.91 -42.36 3.6 0.7] [17.20 -43.24 3.3 0.4] [17.62 -43.00 1.9 0.4] [17.79 -40.13 3.0 0.4]
            [17.71 -39.03 2.4 0.15] [17.56 -37.10 1.6 0.15]]
    lines: [[2 1] [1 3] [1 4] [4 0] [0 5] [5 6] [6 7] [7 8] [8 9] [9 10] [10 11] [11 12] [12 13]]}
   {name: "Lyra"
    stars: [[18.62 38.78 0.0 0.25] [18.83 33.36 3.5 0.3] [18.98 32.69 3.3 0.2] [18.91 36.90 4.3 0.8] [18.75 37.61 4.3 0.3]]
    lines: [[0 4] [4 1] [1 2] [2 3] [3 4]]}])

(def cycle 15.0)

(def (radians degrees) (* degrees (/ math/pi 180)))

(def (project star ra0 dec0)
  "Where star lands on a flat chart centered on ra0 and dec0, east to the left."
  (let [[hours degrees] star
        ra (radians (* 15 hours))
        dec (radians degrees)
        a0 (radians (* 15 ra0))
        d0 (radians dec0)
        across (- ra a0)
        down (+ (* (math/sin d0) (math/sin dec)) (* (math/cos d0) (math/cos dec) (math/cos across)))]
    [(- (/ (* (math/cos dec) (math/sin across)) down))
     (/ (- (* (math/cos d0) (math/sin dec)) (* (math/sin d0) (math/cos dec) (math/cos across))) down)]))

(def (tint warmth)
  (cond [(< warmth 0.2) "#b8ccff"]
    [(< warmth 0.45) "#eef2ff"]
    [(< warmth 0.75) "#ffe7c4"]
    [else "#ffb27a"]))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        index (mod (int (/ seconds cycle)) (count figures))
        into (- seconds (* cycle (int (/ seconds cycle))))
        figure (nth figures index)
        ras (map (fn [s] (nth s 0)) figure.stars)
        decs (map (fn [s] (nth s 1)) figure.stars)
        ra0 (/ (reduce + ras) (count ras))
        dec0 (/ (reduce + decs) (count decs))
        flat (vec (map (fn [s] (project s ra0 dec0)) figure.stars))
        reach (reduce max 0.1 (map (fn [[x y]] (max (abs x) (abs y))) flat))
        scale (/ (* 0.36 frame.height) reach)
        spin (* 0.004 seconds)
        cx (+ (/ frame.width 2.0) (* 0.06 frame.width (math/sin (+ (* 0.2 seconds) index))))
        cy (/ frame.height 2.0)
        placed (vec (map (fn [[x y]]
                           (let [rx (- (* x (math/cos spin)) (* y (math/sin spin)))
                                 ry (+ (* x (math/sin spin)) (* y (math/cos spin)))]
                             [(+ cx (* scale rx)) (- cy (* scale ry))]))
                         flat))
        shown (* (min 1 (/ into 1.5)) (min 1 (max 0 (/ (- cycle into) 1.5))))
        drawn (* (count figure.lines) (min 1 (max 0 (/ (- into 1.5) 4.5))))
        label (* shown (min 1 (max 0 (- into 6.0))))
        lines (vec (keep (fn [k]
                           (let [[a b] (nth figure.lines k)
                                 [x1 y1] (nth placed a)
                                 [x2 y2] (nth placed b)
                                 part (min 1 (max 0 (- drawn k)))]
                             (when (> part 0)
                               [:line x1 y1 (+ x1 (* part (- x2 x1))) (+ y1 (* part (- y2 y1))) ["#8fb3ff" (* 0.35 shown)] 1.2])))
                         (range (count figure.lines))))
        stars (vec (mapcat (fn [k]
                             (let [[x y] (nth placed k)
                                   [_ _ magnitude warmth] (nth figure.stars k)
                                   size (+ 5 (* 4 (- 4.5 magnitude)))
                                   color (tint warmth)
                                   twinkle (+ 0.85 (* 0.15 (math/sin (+ (* 3 seconds) (* 7 k)))))]
                               [[:glow x y (* 2.2 size) [color (* 0.45 shown twinkle)]]
                                [:glow x y size [color (* 0.9 shown)]]
                                [:circle x y (+ 0.8 (* 0.35 (- 4.5 magnitude))) ["#ffffff" (* 0.95 shown)]]]))
                           (range (count figure.stars))))
        [lx ly] (reduce (fn [[bx by] [x y]] (if (> y by) [x y] [bx by])) [0 0] placed)]
    (vec (concat lines stars
                 [[:text (- lx 40) (+ ly 28) figure.name ["#c9d6ff" (* 0.8 label)] (* 0.035 frame.height)]]))))

(test "a constellation's stars land around the middle of its chart"
  (let [orion (first figures)
        flat (map (fn [s] (project s 5.6 0.0)) orion.stars)]
    (expect (every? (fn [[x y]] (and (< (abs x) 0.3) (< (abs y) 0.3))) flat))))

(test "east is to the left on the chart"
  (let [[x _] (project [6.0 0.0] 5.0 0.0)]
    (expect (< x 0))))
