(module
  (import (ox math) (screensaver noise :as noise) (screensaver outline :as outline))
  (export background bubble draw))

(def background "#03202b")
(def bubbles 38)

(def (bubble i seconds width height)
  "Bubble i: rising, wobbling and popping near the top, then rising again."
  (let [size (+ 8 (* 26 (noise/of [i :size])))
        rise (+ 30 (* 2.2 size))
        top (* height (+ 0.08 (* 0.3 (noise/of [i :top]))))
        travel (- (+ height size) top)
        span (+ (/ travel rise) 0.6)
        shifted (+ seconds (* span (noise/of [i :phase])))
        round (int (/ shifted span))
        age (- shifted (* round span))
        risen (* age rise)
        x0 (* width (noise/of [i round :x]))]
    {x: (+ x0 (* (* 0.4 size) (math/sin (+ (* 2.1 age) (* 6 (noise/of [i :wobble]))))))
     y: (- (+ height size) (min risen travel))
     size: (* size (+ 1 (* 0.04 (math/sin (* 5 age)))))
     popped: (when (> risen travel) (/ (- risen travel) rise))}))

(def (shapes b)
  (if b.popped
    (if (< b.popped 0.35)
      (let [spread (* b.size (+ 1 (* 3 b.popped)))
            fade (- 1 (/ b.popped 0.35))]
        (vec (concat (outline/ellipse b.x b.y spread spread ["#d8fbff" (* 0.6 fade)] 1 20)
                     (map (fn [k]
                            (let [angle (* k 1.05)]
                              [:circle (+ b.x (* spread 1.2 (math/cos angle))) (+ b.y (* spread 1.2 (math/sin angle)))
                               1.5 ["#d8fbff" fade]]))
                          (range 6)))))
      [])
    (vec (concat [[:circle b.x b.y b.size ["#7fe7ff" 0.07]]]
                 (outline/ellipse b.x b.y b.size b.size ["#bff4ff" 0.55] 1.2 28)
                 [[:circle (- b.x (* 0.35 b.size)) (- b.y (* 0.38 b.size)) (* 0.22 b.size) ["#ffffff" 0.75]]
                  [:circle (+ b.x (* 0.3 b.size)) (+ b.y (* 0.34 b.size)) (* 0.09 b.size) ["#ffffff" 0.35]]]))))

(def (depths width height)
  (vec (map (fn [band] [:rect 0 (* height 0.1 band) width (+ 1 (* height 0.1)) ["#000814" (* 0.07 band)]])
            (range 10))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)]
    (vec (concat (depths frame.width frame.height)
                 (mapcat (fn [i] (shapes (bubble i seconds frame.width frame.height))) (range bubbles))))))

(test "bubbles rise until they pop"
  (let [early (bubble 7 2.0 800 600)
        later (bubble 7 2.3 800 600)]
    (expect (or later.popped early.popped (< later.y early.y)))))
