(module
  (import (ox math) (screensaver noise :as noise))
  (export background flake drift draw))

(def background "#0b1220")
(def flakes 260)

(def (flake i seconds width height)
  "Where flake i is: near ones are bigger and faster, and all sway on the breeze."
  (let [near (noise/of [i :near])
        speed (+ 18 (* 70 near))
        span (+ height 20)
        y (- (mod (+ (* span (noise/of [i :y])) (* speed seconds)) span) 10)
        breeze (* 14 (math/sin (* 0.11 seconds)))
        x (mod (+ (* width (noise/of [i :x]))
                  (* breeze (+ 0.5 near) seconds 0.1)
                  (* (+ 6 (* 10 near)) (math/sin (+ (* seconds (+ 0.6 near)) (* 9 (noise/of [i :sway]))))))
               width)]
    {x: x y: y radius: (+ 0.7 (* 2.6 near)) alpha: (+ 0.35 (* 0.6 near))}))

(def (drift x seconds height)
  "How deep the snow has drifted at x."
  (* (min 1 (/ seconds 60.0)) height
     (+ 0.05 (* 0.03 (math/sin (* 0.009 x))) (* 0.015 (math/sin (+ 2 (* 0.031 x)))))))

(def (bank seconds width height)
  (vec (map (fn [x]
              (let [deep (drift x seconds height)]
                [:line x (- height deep) x height ["#e6eef8" 0.9] 4]))
            (range 0 (+ width 4) 4))))

(def (draw frame)
  (let [seconds (+ 12 (/ frame.time 1000.0))]
    (vec (concat (map (fn [i]
                        (let [f (flake i seconds frame.width frame.height)]
                          [:circle f.x f.y f.radius ["#f4f8ff" f.alpha]]))
                      (range flakes))
                 (bank seconds frame.width frame.height)))))

(test "snow falls and the drift deepens"
  (let [a (flake 5 1.0 800 600)
        b (flake 5 1.2 800 600)]
    (expect (or (> b.y a.y) (< b.y 20)))
    (expect (< (drift 100 5 600) (drift 100 50 600)))))
