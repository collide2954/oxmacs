(module
  (import (ox math) (screensaver noise :as noise))
  (export background blob field draw))

(def background "#1a0624")
(def blobs 7)
(def cell 26)

(def (blob i seconds width height)
  "Blob i: drifting up and down the lamp, swelling and shrinking as it goes."
  (let [rate (+ 0.05 (* 0.06 (noise/of [i :rate])))
        sway (+ 0.04 (* 0.05 (noise/of [i :sway])))]
    {x: (* width (+ 0.15 (* 0.7 (noise/of [i :x])) (* 0.06 (math/sin (+ (* sway seconds) i)))))
     y: (* height (+ 0.5 (* 0.42 (math/sin (+ (* rate seconds) (* 6 (noise/of [i :phase])))))))
     size: (* height (+ 0.08 (* 0.06 (noise/of [i :size]))) (+ 1 (* 0.12 (math/sin (+ (* 0.7 seconds) i)))))}))

(def (field found x y)
  "How much of the blobs reaches x y; 1 is a blob's edge."
  (reduce (fn [sum b]
            (let [dx (- x b.x) dy (- y b.y)]
              (+ sum (/ (* b.size b.size) (+ 1 (* dx dx) (* dy dy))))))
          0 found))

(def (warmth level)
  (cond
    [(> level 2.2) "#ffe29a"]
    [(> level 1.5) "#ffb057"]
    [(> level 1.1) "#ff7a45"]
    [else "#ff4f6d"]))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        found (vec (map (fn [i] (blob i seconds frame.width frame.height)) (range blobs)))
        columns (+ 1 (int (/ frame.width cell)))
        rows (+ 1 (int (/ frame.height cell)))]
    (vec (keep (fn [index]
                 (let [x (* cell (+ 0.5 (mod index columns)))
                       y (* cell (+ 0.5 (int (/ index columns))))
                       level (field found x y)]
                   (when (> level 0.7)
                     [:circle x y (* 0.68 cell) [(warmth level) (min 1 (* 1.8 (- level 0.7)))]])))
               (range (* columns rows))))))

(test "each blob is solid at its middle and gone far away"
  (let [b (blob 2 10 960 540)]
    (expect (> (field [b] b.x b.y) 1))
    (expect (< (field [b] (+ b.x 900) b.y) 0.1))))
