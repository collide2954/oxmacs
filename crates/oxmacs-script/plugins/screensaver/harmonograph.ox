(module
  (import (ox math) (screensaver noise :as noise))
  (export background swing curve draw))

(def background "#0a0912")
(def cycle 26.0)
(def drawing 20.0)
(def steps 1600)
(def reach 85.0)
(def ratios [[2 3] [3 2] [3 4] [4 3] [1 2] [2 1] [5 4] [4 5] [3 5]])
(def colors ["#ff7aa2" "#ffb86b" "#ffe66d" "#7bdff2" "#b28dff"])

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

(defonce drawn (atom {k: nil points: []}))

(def (points k)
  (when (not= (get @drawn :k) k)
    (reset! drawn {k: k points: (curve k)}))
  (get @drawn :points))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        k (int (/ seconds cycle))
        into-cycle (- seconds (* k cycle))
        shown (int (* steps (min 1 (/ into-cycle drawing))))
        fade (min 1 (/ (- cycle into-cycle) 2.0))
        scale (/ (* 0.42 (min frame.width frame.height)) 1.5)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        spin (* 0.04 seconds)
        turned (fn [[x y]]
                 [(+ cx (* scale (- (* x (math/cos spin)) (* y (math/sin spin)))))
                  (+ cy (* scale (+ (* x (math/sin spin)) (* y (math/cos spin)))))])
        pen (vec (map turned (take (inc shown) (points k))))]
    (vec (concat
          (map (fn [i]
                 (let [[x1 y1] (nth pen i)
                       [x2 y2] (nth pen (inc i))
                       color (nth colors (int (/ (* i (count colors)) (inc steps))))]
                   [:line x1 y1 x2 y2 [color (* 0.85 fade)] 1.2]))
               (range (max 0 (dec (count pen)))))
          (if (and (< into-cycle drawing) (> (count pen) 0))
            (let [[x y] (last pen)] [[:circle x y 3 ["#ffffff" 0.9]]])
            [])))))

(test "the pendulums die down"
  (let [points (curve 4)
        size (fn [[x y]] (+ (abs x) (abs y)))]
    (expect (= (count points) (inc steps)))
    (expect (< (reduce + (map size (take 50 (drop 1500 points))))
               (reduce + (map size (take 50 points)))))))

(test "each drawing swings differently"
  (expect (not= (swing 1) (swing 2))))
