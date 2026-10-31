(module
  (import (ox math) (screensaver noise :as noise))
  (export background flame draw))

(def background "#07040a")
(def flames 170)
(def sparks 30)

(def (flame i seconds)
  "Flame i: where it is above the fire's base, as fractions, its size and how far through its life."
  (let [life (+ 0.55 (* 0.6 (noise/of [i :life])))
        shifted (+ seconds (* life (noise/of [i :born])))
        round (int (/ shifted life))
        age (/ (- shifted (* round life)) life)
        side (- (noise/of [i round :side]) 0.5)]
    {x: (+ (* side 0.5 (- 1 (* 0.7 age))) (* 0.05 age (math/sin (+ (* 9 seconds) (* 12 (noise/of [i :flick]))))))
     y: (* age (+ 0.45 (* 0.35 (noise/of [i round :rise]))))
     size: (* (- 1 age) (+ 0.6 (* 0.6 (noise/of [i round :size]))))
     age: age}))

(def (heat age)
  (cond
    [(< age 0.18) "#fff4c2"]
    [(< age 0.38) "#ffd166"]
    [(< age 0.6) "#ff8c42"]
    [else "#e63946"]))

(def (spark i seconds cx base reach)
  (let [life (+ 1.6 (* 1.4 (noise/of [i :life])))
        shifted (+ seconds (* life (noise/of [i :born])))
        round (int (/ shifted life))
        age (/ (- shifted (* round life)) life)
        x (+ cx (* reach (+ (* 0.6 (- (noise/of [i round :x]) 0.5)) (* 0.3 age (math/sin (+ (* 3 seconds) i))))))
        y (- base (* reach 2.2 age))]
    [:circle x y 1.4 ["#ffcf70" (* 0.9 (- 1 age))]]))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        base (* 0.78 frame.height)
        reach (* 0.32 frame.height)
        glow (+ 0.85 (* 0.15 (math/sin (* 7 seconds))))]
    (vec (concat
          (map (fn [k] [:circle cx (+ base 10) (* reach (- 1.6 (* 0.25 k))) ["#ff7b29" (* glow 0.035 (inc k))]]) (range 5))
          [[:line (- cx (* 0.55 reach)) (+ base 18) (+ cx (* 0.5 reach)) (- base 4) "#3b2418" 16]
           [:line (- cx (* 0.5 reach)) (- base 4) (+ cx (* 0.55 reach)) (+ base 18) "#2e1c12" 16]]
          (map (fn [i]
                 (let [f (flame i seconds)]
                   [:circle (+ cx (* reach f.x)) (- base (* reach 1.6 f.y)) (* reach 0.16 f.size)
                    [(heat f.age) (* 0.55 (- 1 f.age))]]))
               (range flames))
          (map (fn [i] (spark i seconds cx base reach)) (range sparks))))))

(test "flames rise and shrink as they burn"
  (let [f (flame 9 3.0)
        g (flame 9 3.05)]
    (expect (or (< g.age f.age) (and (> g.y f.y) (< g.size f.size))))))
