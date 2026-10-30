(module (import (ox rodeo) (ox math) (ox random) (ox geometry :as v)))

(rodeo/screen 640 480 :smooth true)
(rodeo/actions {feed: [:f] night: [:n]})

(def W 640)
(def H 480)
(def sand 410)
(def tau 6.28318)
(def seed (random/new 42))

(defonce light (atom 1))
(defonce dark (atom false))

(def (mix a b t)
  (vec (map (fn [x y] (+ x (* t (- y x)))) a b)))

(def (lit [r g b a])
  (let [k (+ 0.16 (* 0.84 @light))]
    [(* r k) (* g (+ k 0.04)) (* b (min 1 (+ k 0.24))) (or a 255)]))

(def (water)
  (each [i (range 48)]
    (rodeo/rect [0 (* i 10) W 10] (lit (mix [96 184 202] [6 36 58] (/ i 47))))))

(def (wave x t)
  (+ 28 (* 2.5 (math/sin (+ (* x 0.025) (* t 1.6)))) (math/sin (- (* x 0.07) (* t 2.3)))))

(def (surface t)
  (let [top (for [x (range 0 (+ W 16) 16)] [x (wave x t)])]
    (rodeo/rect [0 0 W 32] (lit [196 218 226]))
    (rodeo/polygon (into top [[W 44] [0 44]]) (lit [106 188 206]))
    (each [[x y] top]
      (rodeo/line [x y] [(+ x 16) (wave (+ x 16) t)] (lit [236 250 255 170]) :width 1.5))))

(def (rays t)
  (each [i (range 7)]
    (let [x (+ 20 (* i 96) (* 20 (math/sin (+ (* 0.25 t) i))))
          w (+ 18 (* 12 (math/sin (+ (* 0.4 t) (* 3 i)))))]
      (rodeo/polygon [[x 32] [(+ x w) 32] [(+ x w 110) sand] [(+ x 70) sand]]
                     [255 255 220 (* @light (+ 12 (* 6 (math/sin (+ t i)))))]))))

(def dunes
  (for [x (range 0 (+ W 32) 32)]
    [x (+ sand (* 7 (math/sin (* x 0.011))) (* 3 (math/sin (+ 1 (* x 0.043)))))]))

(def pebbles
  (for [_ (range 260)]
    [(random/range 0 W :from seed) (random/range (+ sand 14) H :from seed)
     (random/range 0.8 2.6 :from seed)
     (random/pick [[150 128 96] [224 208 170] [116 102 86] [184 164 124]] :from seed)]))

(def (bed t)
  (rodeo/polygon (into dunes [[W H] [0 H]]) (lit [198 176 132]))
  (each [i (range 10)]
    (let [x (mod (+ (* i 71) (* 9 t)) W)]
      (rodeo/circle [x (+ sand 10 (* 3 (math/sin (+ t i))))] (+ 9 (* 4 (math/sin (+ (* 2 t) i))))
                    [255 250 214 (* 40 @light)]))))

(def (rock [cx cy] r)
  (for [i (range 12)]
    (let [a (* i (/ tau 12))
          k (random/range 0.75 1.1 :from seed)]
      [(+ cx (* r k (math/cos a))) (+ cy (* r 0.7 k (math/sin a)))])))

(def rocks [(rock [112 416] 60) (rock [174 424] 38) (rock [526 410] 72) (rock [592 428] 42)])

(def (toward points k lift)
  (let [mid (v/scale (apply v/add points) (/ 1 (count points)))]
    (for [p points] (v/add (v/lerp mid p k) [(* -0.7 lift) (- lift)]))))

(def (stones)
  (each [r rocks]
    (rodeo/polygon r (lit [64 68 74]))
    (rodeo/polygon (toward r 0.8 6) (lit [96 100 104]))
    (rodeo/polygon (toward r 0.45 14) (lit [132 136 132]))))

(def (front-sand)
  (rodeo/polygon (into (for [[x y] dunes] [x (+ y 24)]) [[W H] [0 H]]) (lit [172 150 110]))
  (each [[x y r c] pebbles] (rodeo/circle [x y] r (lit c))))

(def weeds
  (for [_ (range 26)]
    {x: (random/range 0 W :from seed) h: (random/range 90 280 :from seed)
     lean: (random/range -0.25 0.25 :from seed) phase: (random/range 0 tau :from seed)
     green: (random/range 0.5 1 :from seed)}))

(def (blade p t far)
  (let [n 10
        spine (for [i (range (inc n))]
                (let [s (/ i n)
                      sway (* 0.18 s (math/sin (+ (* 0.8 t) p.phase (* 2.5 s))))]
                  [(+ p.x (* s p.h (+ p.lean sway))) (- (+ sand 8) (* s p.h))]))
        edge (fn [side i] (v/add (nth spine i) [(* side 5 (- 1 (/ i (inc n)))) 0]))
        green [(* 44 p.green) (* 150 p.green) (* 72 p.green)]]
    (rodeo/polygon (into (for [i (range (inc n))] (edge -1 i))
                         (for [i (reverse (range (inc n)))] (edge 1 i)))
                   (lit (mix green [30 90 110] far)))))

(def kinds
  {tetra: {size: 18 tall: 0.3 color: [178 198 212] speed: 1.5 beat: 11}
   angel: {size: 44 tall: 0.95 color: [214 214 204] speed: 0.6 beat: 5}
   gourami: {size: 34 tall: 0.5 color: [232 118 60] speed: 0.8 beat: 7}
   cory: {size: 24 tall: 0.36 color: [170 160 140] speed: 0.5 beat: 9}})

(def (half q)
  (+ (* 0.9 (math/sin (* 3.14159 (math/sqrt (math/clamp q 0 1))))) (* 0.14 q)))

(def (place f points)
  (let [c (math/cos f.tilt)
        s (math/sin f.tilt)
        [x y] f.at]
    (for [[u w] points]
      [(+ x (* f.face (- (* u c) (* w s)))) (+ y (* u s) (* w c))])))

(def (fish f t)
  (let [k (get kinds f.kind)
        L k.size
        hh (* L k.tall)
        wag (* 0.22 L (math/sin (+ (* t k.beat) f.phase)))
        tip (* 1.5 wag)
        sail (if (= f.kind :angel) 2 1.3)
        reach (if (= f.kind :angel) -0.32 -0.12)
        at (fn [q w] [(* L (- 0.5 (* 0.82 q))) (+ (* w hh (half q)) (* wag q q))])
        band (fn [q0 q1 w0 w1]
               (let [qs (for [i (range 7)] (+ q0 (* (- q1 q0) (/ i 6))))]
                 (place f (into (for [q qs] (at q w0)) (for [q (reverse qs)] (at q w1))))))
        paint (fn [q0 q1 w0 w1 color] (rodeo/polygon (band q0 q1 w0 w1) (lit color)))
        fin (lit (conj (mix k.color [255 255 255] 0.3) 170))]
    (rodeo/polygon (place f [[(* -0.3 L) (- wag (* 0.12 hh))] [(* -0.62 L) (- tip (* 0.7 hh))]
                             [(* -0.5 L) tip] [(* -0.62 L) (+ tip (* 0.7 hh))]
                             [(* -0.3 L) (+ wag (* 0.12 hh))]])
                   fin)
    (rodeo/polygon (place f [[(* 0.2 L) (* -0.8 hh)] [(* reach L) (* sail -1 hh)] [(* -0.38 L) (* -0.4 hh)]]) fin)
    (rodeo/polygon (place f [[(* 0.06 L) (* 0.8 hh)] [(* reach L) (* sail hh)] [(* -0.38 L) (* 0.4 hh)]]) fin)
    (paint 0 1 -1 1 k.color)
    (paint 0.05 0.95 -1 -0.4 (mix k.color [20 30 40] 0.35))
    (paint 0.05 0.9 0.3 1 (mix k.color [255 255 255] 0.45))
    (match f.kind
      [:tetra (paint 0.12 0.78 -0.45 -0.05 [70 230 255]) (paint 0.45 0.9 0.05 0.75 [235 40 60])]
      [:angel (each [q [0.2 0.45 0.68]] (paint q (+ q 0.08) -1 1 [40 40 44 220]))]
      [:gourami (each [q [0.2 0.38 0.56 0.72]] (paint q (+ q 0.07) -0.9 0.9 [70 120 230 200]))]
      [:cory (each [q [0.2 0.35 0.5 0.65]] (paint q (+ q 0.05) -0.6 -0.2 [60 56 50]))]
      [_ nil])
    (rodeo/polygon (band 0 1 -1 1) [10 20 30 60] :stroke 1)
    (let [eye (first (place f [[(* 0.34 L) (* -0.15 hh)]]))]
      (rodeo/circle eye (max 1.4 (* 0.07 L)) (lit [240 240 236]))
      (rodeo/circle eye (max 0.9 (* 0.045 L)) (lit [10 12 16])))))

(def (fish-of kind x y)
  {kind: kind at: [x y] v: [(random/range -1 1 :from seed) 0] face: 1 tilt: 0
   phase: (random/range 0 tau :from seed) goal: [x y]})

(defonce school
  (atom (into (for [i (range 16)] (fish-of :tetra (+ 300 (* 8 i)) (+ 180 (* 7 (mod i 4)))))
              [(fish-of :angel 200 170) (fish-of :angel 440 240)
               (fish-of :gourami 330 300) (fish-of :cory 250 396) (fish-of :cory 420 400)])))

(defonce food (atom []))
(defonce bubbles (atom []))

(def (pick-goal f)
  (let [bottom (= f.kind :cory)
        flake (first (sort-by (fn [p] (v/distance f.at p.at)) @food))]
    (cond
      [(and flake (< (v/distance f.at flake.at) 160)) (assoc f :goal flake.at)]
      [(or (< (v/distance f.at f.goal) 16) (random/chance? 0.005 :from seed))
       (assoc f :goal [(random/range 40 600 :from seed)
                       (if bottom (random/range 392 402 :from seed) (random/range 80 360 :from seed))])]
      [else f])))

(def (flock f all)
  (if (not= f.kind :tetra)
    [0 0]
    (let [mates (filter (fn [o] (and (= o.kind :tetra) (< (v/distance f.at o.at) 70))) all)
          share (/ 1 (count mates))
          center (v/scale (apply v/add (map (fn [o] o.at) mates)) share)
          heading (v/scale (apply v/add (map (fn [o] o.v) mates)) share)
          apart (apply v/add [0 0] (for [o mates
                                         :let [d (v/distance f.at o.at)]
                                         :when (and (> d 0) (< d 18))]
                                     (v/scale (v/sub f.at o.at) (/ 1 d))))]
      (v/add (v/scale (v/sub center f.at) 0.002) (v/scale (v/sub heading f.v) 0.05) (v/scale apart 0.14)))))

(def (swim f all)
  (let [k (get kinds f.kind)
        f (pick-goal f)
        pull (v/scale (v/normalize (v/sub f.goal f.at)) (* 0.04 k.speed))
        [vx vy] (v/clamp-length (v/scale (v/add f.v pull (flock f all)) 0.97) k.speed)]
    (assoc f
           :at (v/clamp [20 60 600 344] (v/add f.at [vx vy]))
           :v [vx vy]
           :face (+ f.face (* 0.12 (- (if (< vx 0) -1 1) f.face)))
           :tilt (+ f.tilt (* 0.1 (- (math/clamp (* 0.6 (math/atan2 vy (abs vx))) -0.5 0.5) f.tilt))))))

(def (eaten? p all)
  (some (fn [f] (< (v/distance (first (place f [[(* 0.45 (get-in kinds [f.kind :size])) 0]])) p.at) 7)) all))

(def stone [92 398])

(def (flake)
  {at: [(random/range 200 460 :from seed) 34] sink: (random/range 0.12 0.28 :from seed)
   phase: (random/range 0 tau :from seed)})

(def (drift p)
  (let [[x y] p.at]
    (assoc p :at [(+ x (* 0.25 (math/sin (+ p.phase (* 0.05 y))))) (min (+ sand 8) (+ y p.sink))])))

(def (bubble)
  {at: (v/add stone [(random/range -3 3 :from seed) 0]) r: (random/range 1 2.6 :from seed) rise: 0.3})

(def (lift b)
  (let [[x y] b.at]
    (assoc b :at [(+ x (* 0.35 (math/sin (* 0.08 y)))) (- y b.rise)] :rise (min 2.2 (+ b.rise 0.025))
           :r (+ b.r 0.006))))

(def (tick)
  (when (rodeo/pressed? :night) (swap! dark not))
  (when (rodeo/pressed? :feed) (swap! food into (for [_ (range 20)] (flake))))
  (swap! light (fn [l] (+ l (* 0.015 (- (if @dark 0 1) l)))))
  (let [all @school]
    (reset! school (vec (for [f all] (swim f all))))
    (swap! food (fn [flakes] (vec (for [p flakes :when (not (eaten? p all))] (drift p))))))
  (when (random/chance? 0.3 :from seed) (swap! bubbles conj (bubble)))
  (swap! bubbles (fn [all] (vec (for [b all :when (> (second b.at) 34)] (lift b))))))

(def (air)
  (rodeo/line stone [(first stone) 0] [40 60 50 200] :width 2)
  (rodeo/rect [(- (first stone) 8) (- (second stone) 3) 16 7] (lit [90 80 74]))
  (each [b @bubbles]
    (rodeo/circle b.at b.r (lit [220 242 255 170]) :stroke 1)
    (rodeo/circle (v/sub b.at [(* 0.35 b.r) (* 0.35 b.r)]) (* 0.3 b.r) [255 255 255 200])))

(def (flakes)
  (each [p @food]
    (let [[x y] p.at]
      (rodeo/polygon [[x (- y 2)] [(+ x 3) y] [x (+ y 2)] [(- x 3) y]] (lit [206 98 44])))))

(def (glass)
  (rodeo/polygon [[30 0] [96 0] [-24 H] [-90 H]] [255 255 255 7])
  (rodeo/polygon [[120 0] [134 0] [14 H] [0 H]] [255 255 255 6])
  (rodeo/rect [0 0 W 7] [22 24 28])
  (rodeo/rect [0 (- H 6) W 6] [22 24 28]))

(def (draw)
  (let [t (rodeo/time)]
    (water)
    (rays t)
    (each [p (take 12 weeds)] (blade p t 0.45))
    (bed t)
    (stones)
    (front-sand)
    (each [p (drop 12 weeds)] (blade p t 0))
    (each [f @school] (fish f t))
    (flakes)
    (air)
    (surface t)
    (glass)))
