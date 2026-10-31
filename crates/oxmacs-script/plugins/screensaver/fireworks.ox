(module
  (import (ox math) (screensaver noise :as noise))
  (export background rocket draw))

(def background "#04040c")
(def palette ["#ff5d73" "#ffd166" "#06d6a0" "#4cc9f0" "#f72585" "#b8f2e6" "#ff9f1c" "#c77dff"])
(def period 0.6)
(def rise 1.2)
(def burn 2.4)
(def gravity 110.0)

(def (rocket i width height)
  "Rocket i: when it goes up, where it bursts and how."
  {id: i
   launch: (* period (+ i (* 0.5 (noise/of [i :when]))))
   x: (* width (+ 0.12 (* 0.76 (noise/of [i :x]))))
   ground: height
   peak: (* height (+ 0.12 (* 0.33 (noise/of [i :peak]))))
   color: (nth palette (int (* (count palette) (noise/of [i :color]))))
   sparks: (+ 48 (int (* 30 (noise/of [i :sparks]))))
   speed: (* height (+ 0.26 (* 0.16 (noise/of [i :speed]))))})

(def (place r k dt)
  (let [angle (+ (* 2 math/pi (/ k r.sparks)) (* 0.3 (noise/of [r.id k :angle])))
        pace (* r.speed (+ 0.7 (* 0.3 (noise/of [r.id k :pace]))))
        drift (/ (- 1 (math/exp (* -1.6 dt))) 1.6)]
    [(+ r.x (* pace (math/cos angle) drift))
     (+ r.peak (* pace (math/sin angle) drift) (* 0.5 gravity dt dt))]))

(def (spark r k dt fade)
  (let [[x y] (place r k dt)
        [tx ty] (place r k (max 0 (- dt 0.2)))
        twinkle (+ 0.65 (* 0.35 (math/sin (+ (* 37 dt) k))))]
    [[:line tx ty x y [r.color (* 0.6 fade)] 1.8]
     [:circle x y 2.2 [r.color (* fade twinkle)]]]))

(def (shell r seconds)
  (let [age (- seconds r.launch)]
    (cond
      [(< age 0) []]
      [(< age rise)
       (let [height-at (fn [a] (- r.ground (* (- r.ground r.peak) (- 1 (math/pow (- 1 (/ a rise)) 2)))))
             y (height-at age)
             trail (height-at (max 0 (- age 0.12)))]
         [[:line r.x trail r.x y ["#ffe9c4" 0.55] 1.5]
          [:circle r.x y 1.8 "#fff4e0"]])]
      [(< age (+ rise burn))
       (let [dt (- age rise)
             fade (math/pow (- 1 (/ dt burn)) 1.3)
             flash (if (< dt 0.15) [[:circle r.x r.peak (* 70 (- 1 (/ dt 0.15))) [r.color 0.18]]] [])]
         (vec (concat flash (mapcat (fn [k] (spark r k dt fade)) (range r.sparks)))))]
      [else []])))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        oldest (max 0 (- (int (/ (- seconds rise burn) period)) 1))]
    (vec (mapcat (fn [i] (shell (rocket i frame.width frame.height) seconds))
                 (range oldest (+ 2 (int (/ seconds period))))))))

(test "rockets burst high above the ground"
  (expect (every? (fn [i]
                    (let [r (rocket i 800 600)]
                      (< r.peak (* 0.5 r.ground))))
                  (range 50))))

(test "sparks fall once they slow down"
  (let [r (rocket 3 800 600)
        [_ early] (place r 0 0.5)
        [_ late] (place r 0 2.0)]
    (expect (> late early))))
