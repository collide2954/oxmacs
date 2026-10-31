(module
  (import (ox math) (ox time) (screensaver shading :as shading))
  (export shader spot draw))

(def wide 0.6)

(def shader
  (str shading/library
       """
       fn shade(at: vec2f) -> vec4f {
           let uv = (at - frame.size * 0.5) / frame.size.y;
           var color = mix(vec3f(0.020, 0.035, 0.055), vec3f(0.006, 0.010, 0.020), smoothstep(-0.6, 0.6, uv.y + uv.x * 0.3));
           for (var i = 0; i < 26; i++) {
               let s = f32(i);
               let speed = 0.004 + 0.01 * cell2(vec2f(s, 1.0));
               let home = vec2f(cell2(vec2f(s, 2.0)) * 2.4 - 1.2, cell2(vec2f(s, 3.0)) * 1.4 - 0.7);
               let place = home + vec2f(sin(frame.time * speed * 6.0 + s), cos(frame.time * speed * 5.0 + s * 1.7)) * 0.25;
               let size = 0.04 + 0.12 * cell2(vec2f(s, 4.0));
               let d = length(uv - place);
               let warm = cell2(vec2f(s, 5.0));
               let tint = mix(vec3f(0.25, 0.45, 0.70), vec3f(0.85, 0.55, 0.30), step(0.7, warm));
               let disc = smoothstep(size, size * 0.6, d) * 0.6 + smoothstep(size, size * 0.97, d) * 0.15;
               color += tint * disc * (0.05 + 0.05 * sin(frame.time * 0.2 + s));
           }
           color *= 1.0 - smoothstep(0.4, 1.2, length(uv)) * 0.6;
           return shown(color * 1.4, at);
       }
       """))

(def (spot seconds free-x free-y)
  "Where the clock drifts to: slowly around the free space, never off screen."
  [(* free-x (+ 0.5 (* 0.5 (math/sin (* 0.07 seconds)))))
   (* free-y (+ 0.5 (* 0.5 (math/sin (+ 1.3 (* 0.053 seconds))))))])

(def (draw frame)
  (let [now (time/now)
        seconds (/ frame.time 1000.0)
        size (* 0.2 frame.height)
        small (* 0.3 size)
        hours (time/format now "%H:%M")
        day (time/format now "%A %e %B")
        width (* wide size (count hours))
        day-width (* wide small (count day))
        box (max width day-width)
        tall (+ (* 1.25 size) (* 1.6 small))
        [x y] (spot seconds (- frame.width box) (- frame.height tall))
        middle (+ x (/ box 2))
        left (- middle (/ width 2))
        under (+ y (* 1.2 size))
        passed (/ (mod now 60000) 60000.0)
        shadow ["#000000" 0.45]]
    [[:glow middle (+ y (* 0.6 size)) (* 1.4 box) ["#000000" 0.35]]
     [:text (+ left 3) (+ y 4) hours shadow size]
     [:text left y hours "#e9eef6" size]
     [:line left under (+ left width) under ["#8aa0bd" 0.25] 2]
     [:line left under (+ left (* width passed)) under ["#c8d8ff" 0.85] 2]
     [:glow (+ left (* width passed)) under 10 ["#c8d8ff" 0.6]]
     [:text (+ (- middle (/ day-width 2)) 2) (+ under (* 0.3 small) 2) day shadow small]
     [:text (- middle (/ day-width 2)) (+ under (* 0.3 small)) day "#9fb3cc" small]]))

(test "the clock stays inside the screen"
  (expect (every? (fn [seconds]
                    (let [[x y] (spot seconds 300 200)]
                      (and (<= 0 x 300) (<= 0 y 200))))
                  (range 0 600 7))))
