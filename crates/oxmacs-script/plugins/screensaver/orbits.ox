(module
  (import (ox math) (screensaver tones :as tones))
  (export spot draw))

(def tilt 0.34)

(def planets
  [{size: 0.09 eccentric: 0.2 turn: 0.4 period: 4 radius: 3.0 color: [0.72 0.69 0.65] moons: 0 ring: false}
   {size: 0.15 eccentric: 0.05 turn: 1.9 period: 7 radius: 5.0 color: [0.91 0.75 0.48] moons: 0 ring: false}
   {size: 0.22 eccentric: 0.03 turn: 3.1 period: 11 radius: 5.5 color: [0.30 0.55 0.90] moons: 1 ring: false}
   {size: 0.30 eccentric: 0.09 turn: 5.0 period: 16 radius: 4.2 color: [0.85 0.40 0.28] moons: 0 ring: false}
   {size: 0.45 eccentric: 0.05 turn: 0.9 period: 28 radius: 12 color: [0.85 0.71 0.55] moons: 3 ring: false}
   {size: 0.60 eccentric: 0.06 turn: 2.6 period: 45 radius: 10 color: [0.89 0.83 0.61] moons: 2 ring: true}
   {size: 0.78 eccentric: 0.04 turn: 4.2 period: 70 radius: 7 color: [0.62 0.85 0.88] moons: 1 ring: false}])

(def shades
  (vec (map (fn [planet]
              (let [dark (tones/blend planet.color [0 0 0] 0.85)
                    light (tones/blend planet.color [1 1 1] 0.2)]
                (vec (map (fn [i] (tones/rgb (tones/blend dark light (/ i 5.0)))) (range 6)))))
            planets)))

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

(def (seen [x y] cx cy)
  [(+ cx x) (+ cy (* tilt y))])

(def (orbit planet reach cx cy)
  (let [a (* reach planet.size)
        b (* a (math/sqrt (- 1 (* planet.eccentric planet.eccentric))))
        c (math/cos planet.turn)
        s (math/sin planet.turn)
        points (vec (map (fn [i]
                           (let [angle (/ (* 2 math/pi i) 96)
                                 x (- (* a (math/cos angle)) (* a planet.eccentric))
                                 y (* b (math/sin angle))]
                             (seen [(- (* x c) (* y s)) (+ (* x s) (* y c))] cx cy)))
                         (range 97)))]
    [:curve points ["#9aa6c8" 0.10] 1]))

(def (trail planet seconds reach cx cy)
  (let [points (vec (map (fn [i] (seen (spot planet (- seconds (* planet.period 0.012 i)) reach) cx cy)) [8 7 6 5 4 3 2 1 0]))]
    [:curve points [(tones/rgb planet.color) 0.35] (* 0.7 planet.radius) 1.0]))

(def (globe k planet x y cx cy)
  (let [r planet.radius
        layers (nth shades k)
        dx (- cx x)
        dy (- cy y)
        apart (max 1 (math/sqrt (+ (* dx dx) (* dy dy))))
        lx (/ dx apart)
        ly (/ dy apart)]
    (vec (concat [[:glow x y (* 2.4 r) [(tones/rgb planet.color) 0.18]]]
                 (map (fn [i]
                        (let [t (/ i 5.0)
                              shift (* r 0.45 t)]
                          [:circle (+ x (* lx shift)) (+ y (* ly shift)) (* r (- 1 (* 0.75 t))) (nth layers i)]))
                      (range 6))))))

(def (ring planet x y)
  (let [points (vec (map (fn [i]
                           (let [angle (/ (* 2 math/pi i) 64)]
                             [(+ x (* 2.3 planet.radius (math/cos angle))) (+ y (* 0.75 planet.radius (math/sin angle)))]))
                         (range 65)))]
    [[:curve points ["#e8dcb0" 0.55] 2.2]
     [:curve points ["#fff3cc" 0.25] 5]]))

(def (body k planet seconds reach cx cy)
  (let [[x y] (spot planet seconds reach)
        [px py] (seen [x y] cx cy)
        moons (mapcat (fn [m]
                        (let [angle (+ (* seconds (+ 1.3 m)) (* 2 m))
                              r (+ (* 2.4 planet.radius) (* 5 m))
                              mx (+ px (* r (math/cos angle)))
                              my (+ py (* tilt r (math/sin angle)))]
                          [[:circle mx my 1.6 "#cfd3dc"] [:glow mx my 4 ["#cfd3dc" 0.3]]]))
                      (range planet.moons))]
    {behind: (< y 0)
     shapes: (vec (concat [(trail planet seconds reach cx cy)]
                          (globe k planet px py cx cy)
                          (if planet.ring (ring planet px py) [])
                          moons))}))

(def (sun cx cy reach)
  [[:glow cx cy (* reach 0.42) ["#ff9a3c" 0.22]]
   [:glow cx cy (* reach 0.20) ["#ffc46b" 0.55]]
   [:glow cx cy (* reach 0.09) ["#fff1c9" 0.9]]
   [:circle cx cy (* reach 0.028) "#fffaf0"]])

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        reach (* 0.6 frame.width)
        bodies (map (fn [k] (body k (nth planets k) seconds reach cx cy)) (range (count planets)))]
    (vec (concat (map (fn [planet] (orbit planet reach cx cy)) planets)
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
