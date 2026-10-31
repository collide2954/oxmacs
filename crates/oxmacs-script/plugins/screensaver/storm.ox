(module
  (import (screensaver noise :as noise))
  (export background strike bolt draw))

(def background "#0a0d14")
(def spacing 3.2)
(def lasting 0.32)

(def (strike n)
  "When strike n comes, in seconds."
  (* spacing (+ n (* 0.8 (noise/of [n :when])))))

(def (bolt n x1 y1 x2 y2 depth)
  "A jagged bolt from x1 y1 to x2 y2, its middle pushed aside again and again."
  (if (= depth 0)
    [[x1 y1 x2 y2]]
    (let [mx (+ (/ (+ x1 x2) 2) (* (- (noise/of [n depth x1 :push]) 0.5) 0.45 (abs (- y2 y1))))
          my (/ (+ y1 y2) 2)]
      (vec (concat (bolt n x1 y1 mx my (dec depth)) (bolt n mx my x2 y2 (dec depth)))))))

(def (lit n seconds)
  "How bright strike n is now: it flickers off and on before it fades."
  (let [age (- seconds (strike n))]
    (cond
      [(or (< age 0) (> age lasting)) 0]
      [(< 0.07 age 0.12) 0.15]
      [else (- 1 (/ age lasting))])))

(def (lightning n bright width height)
  (let [x (* width (+ 0.15 (* 0.7 (noise/of [n :x]))))
        ground (* height (+ 0.75 (* 0.2 (noise/of [n :ground]))))
        main (bolt n x (* 0.12 height) (+ x (* width 0.15 (- (noise/of [n :lean]) 0.5))) ground 6)
        [bx by] (let [[_ _ x2 y2] (nth main 20)] [x2 y2])
        branch (bolt (+ n 0.5) bx by (+ bx (* width 0.12 (- (noise/of [n :fork]) 0.3))) (+ by (* 0.25 height)) 5)]
    (vec (concat
          (mapcat (fn [[x1 y1 x2 y2]]
                    [[:line x1 y1 x2 y2 ["#9db8ff" (* 0.3 bright)] 6]
                     [:line x1 y1 x2 y2 ["#f4f7ff" bright] 1.8]])
                  main)
          (map (fn [[x1 y1 x2 y2]] [:line x1 y1 x2 y2 ["#dfe7ff" (* 0.7 bright)] 1]) branch)))))

(def (clouds seconds flash width height)
  (vec (map (fn [i]
              (let [x (mod (+ (* width (noise/of [i :x])) (* 6 seconds)) (+ width 200))]
                [:circle (- x 100) (* height (+ 0.02 (* 0.16 (noise/of [i :y])))) (+ 60 (* 90 (noise/of [i :r])))
                 [(if (> flash 0.3) "#5a6585" "#1c2233") (+ 0.25 (* 0.3 (noise/of [i :a])))]]))
            (range 28))))

(def (rain seconds width height)
  (vec (map (fn [i]
              (let [speed (+ 700 (* 400 (noise/of [i :speed])))
                    y (mod (+ (* height (noise/of [i :y])) (* speed seconds)) (+ height 40))
                    x (mod (+ (* width (noise/of [i :x])) (* 0.3 y)) width)]
                [:line x (- y 40) (- x 12) y ["#8a9bc4" 0.35] 1]))
            (range 260))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        n (int (/ seconds spacing))
        strikes (filter (fn [k] (> (lit k seconds) 0)) [(dec n) n (inc n)])
        flash (reduce max 0 (map (fn [k] (lit k seconds)) strikes))]
    (vec (concat
          (if (> flash 0) [[:rect 0 0 frame.width frame.height ["#c9d6ff" (* 0.22 flash)]]] [])
          (clouds seconds flash frame.width frame.height)
          (mapcat (fn [k] (lightning k (lit k seconds) frame.width frame.height)) strikes)
          (rain seconds frame.width frame.height)))))

(test "strikes come one after another"
  (expect (every? (fn [n] (< (strike n) (strike (inc n)))) (range 100))))

(test "a bolt reaches the ground"
  (let [segments (bolt 1 100 0 120 500 6)
        [_ _ x y] (last segments)]
    (expect (= (count segments) 64))
    (expect (= [x y] [120 500]))))
