(module
  (import (ox math) (screensaver outline :as outline))
  (export background spot draw))

(def background "#03040a")
(def tilt 0.36)

(def planets
  [{size: 0.09 eccentric: 0.2 turn: 0.4 period: 9 radius: 2.5 color: "#b8b2a7" moons: 0}
   {size: 0.15 eccentric: 0.05 turn: 1.9 period: 16 radius: 4 color: "#e8c07a" moons: 0}
   {size: 0.22 eccentric: 0.03 turn: 3.1 period: 26 radius: 4.5 color: "#5fa8e8" moons: 1}
   {size: 0.3 eccentric: 0.09 turn: 5.0 period: 38 radius: 3.5 color: "#d9654b" moons: 0}
   {size: 0.45 eccentric: 0.05 turn: 0.9 period: 70 radius: 9 color: "#d8b68b" moons: 3}
   {size: 0.6 eccentric: 0.06 turn: 2.6 period: 110 radius: 7.5 color: "#e3d39c" moons: 2}
   {size: 0.78 eccentric: 0.04 turn: 4.2 period: 170 radius: 5.5 color: "#9fd8e0" moons: 1}])

(def (anomaly mean eccentric)
  "Kepler's equation, solved by a few Newton steps."
  (loop [e mean k 0]
    (if (= k 5)
      e
      (again (- e (/ (- e (* eccentric (math/sin e)) mean) (- 1 (* eccentric (math/cos e))))) (inc k)))))

(def (spot planet seconds reach)
  "Where planet is in its orbit plane, the star at 0 0."
  (let [a (* reach planet.size)
        b (* a (math/sqrt (- 1 (* planet.eccentric planet.eccentric))))
        e (anomaly (+ (/ (* 2 math/pi seconds) planet.period) planet.turn) planet.eccentric)
        x (* a (- (math/cos e) planet.eccentric))
        y (* b (math/sin e))
        c (math/cos planet.turn)
        s (math/sin planet.turn)]
    [(- (* x c) (* y s)) (+ (* x s) (* y c))]))

(def (orbit planet reach cx cy)
  (let [a (* reach planet.size)
        b (* a (math/sqrt (- 1 (* planet.eccentric planet.eccentric))))
        c (math/cos planet.turn)
        s (math/sin planet.turn)
        points (vec (map (fn [i]
                           (let [angle (/ (* 2 math/pi i) 72)
                                 x (- (* a (math/cos angle)) (* a planet.eccentric))
                                 y (* b (math/sin angle))]
                             [(+ cx (- (* x c) (* y s))) (+ cy (* tilt (+ (* x s) (* y c))))]))
                         (range 73)))]
    (outline/path points ["#8890a8" 0.18] 1)))

(def (body planet seconds reach cx cy)
  (let [[x y] (spot planet seconds reach)
        px (+ cx x)
        py (+ cy (* tilt y))
        moons (map (fn [m]
                     (let [angle (+ (* seconds (+ 1.3 m)) (* 2 m))
                           r (+ (* 2.4 planet.radius) (* 4 m))]
                       [:circle (+ px (* r (math/cos angle))) (+ py (* tilt r (math/sin angle))) 1.4 "#cfd3dc"]))
                   (range planet.moons))]
    {behind: (< y 0)
     shapes: (vec (concat [[:circle px py (* 1.8 planet.radius) [planet.color 0.15]]
                           [:circle px py planet.radius planet.color]]
                          moons))}))

(def (sun cx cy reach)
  (vec (map (fn [k] [:circle cx cy (* reach (- 0.08 (* 0.012 k))) ["#ffcf6b" (+ 0.05 (* 0.12 k))]])
            (range 6))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        reach (* 0.6 frame.width)
        bodies (map (fn [planet] (body planet seconds reach cx cy)) planets)]
    (vec (concat (mapcat (fn [planet] (orbit planet reach cx cy)) planets)
                 (mapcat (fn [b] b.shapes) (filter (fn [b] b.behind) bodies))
                 (sun cx cy reach)
                 (mapcat (fn [b] b.shapes) (filter (fn [b] (not b.behind)) bodies))))))

(test "planets come back around after their period"
  (let [planet (nth planets 2)
        [x1 y1] (spot planet 3 500)
        [x2 y2] (spot planet (+ 3 planet.period) 500)]
    (expect (< (+ (abs (- x1 x2)) (abs (- y1 y2))) 0.01))))

(test "Kepler's equation holds"
  (let [e (anomaly 1.0 0.2)]
    (expect (< (abs (- (- e (* 0.2 (math/sin e))) 1.0)) 0.000001))))
