(module
  (import (ox math) (ox time))
  (export spot draw))

(def wide 0.6)

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
        passed (/ (mod now 60000) 60000.0)]
    [[:text left y hours :foreground size]
     [:line left under (+ left width) under [:comment 0.35] 2]
     [:line left under (+ left (* width passed)) under [:keyword 0.9] 2]
     [:text (- middle (/ day-width 2)) (+ under (* 0.3 small)) day :comment small]]))

(test "the clock stays inside the screen"
  (expect (every? (fn [seconds]
                    (let [[x y] (spot seconds 300 200)]
                      (and (<= 0 x 300) (<= 0 y 200))))
                  (range 0 600 7))))
