(module
  (import (ox math) (screensaver noise :as noise))
  (export swing curve draw))

(def cycle 26.0)
(def drawing 20.0)
(def steps 1600)
(def reach 85.0)
(def ratios [[2 3] [3 2] [3 4] [4 3] [1 2] [2 1] [5 4] [4 5] [3 5]])
(def inks ["#14213d" "#3a1c2b" "#0f3326" "#2a1f45" "#3b2a14"])

(def (swing k)
  "The pendulums of drawing k: frequencies near a simple ratio, phases and how fast they die down."
  (let [[a b] (nth ratios (int (* (count ratios) (noise/of [k :ratio]))))
        detune (fn [key] (* 0.02 (- (noise/of [k key]) 0.5)))]
    {fx: (+ a (detune :fx)) fy: (+ b (detune :fy))
     gx: (+ b (detune :gx)) gy: (+ a (detune :gy))
     px: (* 2 math/pi (noise/of [k :px])) py: (* 2 math/pi (noise/of [k :py]))
     qx: (* 2 math/pi (noise/of [k :qx])) qy: (* 2 math/pi (noise/of [k :qy]))
     damping: (+ 0.008 (* 0.012 (noise/of [k :damping])))}))

(def (curve k)
  "Where the pen of drawing k goes, as steps points from -1.5 to 1.5."
  (let [s (swing k)]
    (vec (map (fn [i]
                (let [t (* reach (/ i steps))
                      decay (math/exp (* -1 s.damping t))
                      slower (math/exp (* -1.4 s.damping t))]
                  [(+ (* decay (math/sin (+ (* s.fx t) s.px))) (* 0.5 slower (math/sin (+ (* s.gx t) s.qx))))
                   (+ (* decay (math/sin (+ (* s.fy t) s.py))) (* 0.5 slower (math/sin (+ (* s.gy t) s.qy))))]))
              (range (inc steps))))))

(defonce drawn (atom {key: nil points: []}))

(def (points k width height)
  (let [key [k width height]]
    (when (not= (get @drawn :key) key)
      (let [scale (/ (* 0.42 (min width height)) 1.5)
            cx (/ width 2.0)
            cy (/ height 2.0)]
        (reset! drawn {key: key
                       points: (vec (map (fn [[x y]] [(+ cx (* scale x)) (+ cy (* scale y))]) (curve k)))})))
    (get @drawn :points)))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        k (int (/ seconds cycle))
        into-cycle (- seconds (* k cycle))
        shown (int (* steps (min 1 (/ into-cycle drawing))))
        fade (min 1 (/ (- cycle into-cycle) 2.0))
        ink (nth inks (mod k (count inks)))
        pen (vec (take (inc shown) (points k frame.width frame.height)))
        tip (if (and (< into-cycle drawing) (> (count pen) 1))
              (let [[x y] (last pen)] [[:circle x y 2.2 [ink 0.9]] [:glow x y 7 [ink 0.25]]])
              [])]
    (if (< (count pen) 2)
      []
      (vec (concat [[:curve pen [ink (* 0.07 fade)] 3.2]
                    [:curve pen [ink (* 0.8 fade)] 1.0]]
                   tip)))))

(test "the pendulums die down"
  (let [points (curve 4)
        size (fn [[x y]] (+ (abs x) (abs y)))]
    (expect (= (count points) (inc steps)))
    (expect (< (reduce + (map size (take 50 (drop 1500 points))))
               (reduce + (map size (take 50 points)))))))

(test "each drawing swings differently"
  (expect (not= (swing 1) (swing 2))))
