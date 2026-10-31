(module
  (import (ox math))
  (export background swing draw))

(def background "#08080f")
(def count-of 18)
(def cycle 60.0)
(def least 30)
(def colors ["#ff595e" "#ff924c" "#ffca3a" "#c5ca30" "#8ac926" "#52a675" "#1982c4" "#4267ac" "#6a4c93"])

(def (swing k seconds)
  "How far pendulum k is from the middle, from -1 to 1: it swings least + k times each cycle, so they fall in and out of step."
  (math/cos (* 2 math/pi (+ least k) (/ seconds cycle))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        top (* 0.06 frame.height)
        reach (* 0.32 frame.width)
        gap (/ (* 0.82 frame.height) count-of)]
    (vec (mapcat (fn [k]
                   (let [y (+ top (* gap (+ k 1)))
                         color (nth colors (int (/ (* k (count colors)) count-of)))
                         x (+ cx (* reach (swing k seconds)))
                         ghosts (map (fn [g]
                                       [:circle (+ cx (* reach (swing k (- seconds (* 0.04 g))))) y (- 9 g)
                                        [color (* 0.1 (- 5 g))]])
                                     (range 1 5))]
                     (vec (concat [[:line cx top x y ["#9aa0b5" 0.12] 1]]
                                  ghosts
                                  [[:circle x y 9 color]
                                   [:circle (- x 3) (- y 3) 2.5 ["#ffffff" 0.6]]]))))
                 (range count-of)))))

(test "the pendulums all line up again after a cycle"
  (expect (every? (fn [k] (< (abs (- (swing k 0) (swing k cycle))) 0.0001)) (range count-of)))
  (expect (some (fn [k] (> (abs (- (swing k 7) (swing 0 7))) 0.5)) (range count-of))))
