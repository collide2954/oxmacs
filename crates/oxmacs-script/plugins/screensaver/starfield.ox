(module
  (import (screensaver noise :as noise))
  (export star draw))

(def tints [:foreground :foreground :foreground :link :keyword :string])

(def field
  (vec (map (fn [i]
              {x: (- (* 2 (noise/of [i :x])) 1)
               y: (- (* 2 (noise/of [i :y])) 1)
               speed: (+ 0.06 (* 0.14 (noise/of [i :speed])))
               phase: (noise/of [i :phase])
               tint: (nth tints (mod i (count tints)))})
            (range 240))))

(def (star i seconds)
  (let [s (nth field i)]
    (assoc s :z (max 0.01 (- 1 (mod (+ s.phase (* seconds s.speed)) 1))))))

(def (spot s z cx cy)
  [(+ cx (/ (* s.x cx 0.5) z))
   (+ cy (/ (* s.y cy 0.5) z))])

(def (glow s cx cy width height)
  (let [[x y] (spot s s.z cx cy)]
    (if (and (<= 0 x width) (<= 0 y height))
      (let [near (- 1 s.z)
            radius (+ 0.5 (* 2.2 near near))
            alpha (min 1 (+ 0.15 (* 1.2 near)))
            dot [:circle x y radius [s.tint alpha]]]
        (if (< s.z 0.45)
          (let [[tx ty] (spot s (min 1 (+ s.z 0.05)) cx cy)]
            [[:line tx ty x y [s.tint (* alpha 0.5)] (* radius 0.9)] dot])
          [dot]))
      [])))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)]
    (mapcat (fn [i] (glow (star i seconds) cx cy frame.width frame.height))
            (range (count field)))))

(test "stars rush toward you"
  (expect (every? (fn [i]
                    (let [a (star i 2.0) b (star i 2.05)]
                      (or (< b.z a.z) (< a.z 0.02))))
                  (range 100))))
