(module
  (import (ox rodeo) (ox math) (ox random)
          (ox collide) (ox string)))

(rodeo/screen 600 640 :smooth true)
(rodeo/actions {drop: [:space :b] strong: [:s]
                weak: [:w] reset: [:r]})

(def ka 1.8e-5)
(def temperature 300)
(def stirring 1)
(def indicator :universal)

(def kw 1e-14)
(def acid-ml 50)
(def drop-ml 2.5)
(def mmol 0.25)
(def acid-count 20)
(def water-count 60)
(def most-drops 32)
(def drip-every 0.25)
(def fall-time 0.3)
(def hop-time 0.35)
(def gain 250)
(def relay 0.04)
(def reach 200)
(def pull 400)
(def liquid [40 112 520 268])
(def chart [64 448 496 144])

(def elements
  {o: [232 87 76] h: [240 240 240] cl: [106 211 90]
   na: [176 124 255] a: [222 150 72]})

(def species
  {water: {name: "H₂O" mass: 18 atoms: [[0 0 6.5 :o] [-5.9 4.6 3.6 :h] [5.9 4.6 3.6 :h]]}
   hydronium: {name: "H₃O⁺" mass: 19 halo: [255 110 90]
               atoms: [[0 0 6.5 :o] [0 7.5 3.6 :h] [-6.5 -3.8 3.6 :h] [6.5 -3.8 3.6 :h]]}
   hydroxide: {name: "OH⁻" mass: 17 halo: [90 170 255] atoms: [[0 0 6.5 :o] [0 7.5 3.6 :h]]}
   hcl: {name: "HCl" mass: 36 atoms: [[0 0 8 :cl] [9.5 0 3.6 :h]]}
   chloride: {name: "Cl⁻" mass: 35 halo: [110 215 95] atoms: [[0 0 8 :cl]]}
   sodium: {name: "Na⁺" mass: 23 halo: [180 130 255] atoms: [[0 0 5 :na]]}
   ha: {name: "HA" mass: 60 atoms: [[0 0 8 :a] [9.5 0 3.6 :h]]}
   anion: {name: "A⁻" mass: 59 halo: [255 190 80] atoms: [[0 0 8 :a]]}})

(def (reactions)
  [{from: [:hcl :water] to: [:chloride :hydronium] chance: 1 text: "HCl + H₂O → Cl⁻ + H₃O⁺"}
   {from: [:hcl :hydroxide] to: [:chloride :water] chance: 1 text: "HCl + OH⁻ → Cl⁻ + H₂O"}
   {from: [:hydronium :hydroxide] to: [:water :water] chance: 1 text: "H₃O⁺ + OH⁻ → 2 H₂O"}
   {from: [:ha :hydroxide] to: [:anion :water] chance: 1 text: "HA + OH⁻ → A⁻ + H₂O"}
   {from: [:ha :water] to: [:anion :hydronium] chance: (min 1 (* ka gain)) text: "HA + H₂O → A⁻ + H₃O⁺"}
   {from: [:hydronium :anion] to: [:water :ha] chance: 0.5 text: "H₃O⁺ + A⁻ → H₂O + HA"}
   {from: [:water :anion] to: [:hydroxide :ha] chance: (min 1 (* (/ kw ka) gain)) text: "H₂O + A⁻ → OH⁻ + HA"}
   {from: [:hydronium :water] to: [:water :hydronium] chance: relay}
   {from: [:water :hydroxide] to: [:hydroxide :water] chance: relay}])

(def attractions [[:hydroxide :hydronium] [:hydroxide :ha] [:hydroxide :hcl] [:hydronium :anion]])

(defonce rng (random/new 2026))
(defonce world (atom nil))

(def (between low high) (random/range low high :from rng))

(def (particle kind x y)
  {kind: kind x: x y: y vx: 0 vy: 0 angle: (between 0 6.28) spin: (between -1 1)})

(def (scatter kind n)
  (let [[left top width height] liquid]
    (for [_i (range n)]
      (particle kind
                (between (+ left 12) (- (+ left width) 12))
                (between (+ top 12) (- (+ top height) 12))))))

(def (equilibrium strong weak base volume)
  "The pH at which the charges balance, for amounts in mmol in volume mL."
  (let [cs (/ strong volume)
        cw (/ weak volume)
        cb (/ base volume)]
    (loop [low -1.0 high 15.0 n 0]
      (let [ph (/ (+ low high) 2)
            h (math/pow 10 (- ph))
            excess (- (+ h cb) (/ kw h) cs (/ (* cw ka) (+ ka h)))]
        (cond ((= n 40) ph)
          ((> excess 0) (again ph high (inc n)))
          (else (again low ph (inc n))))))))

(def (composition state)
  (frequencies (map :kind state.particles)))

(def (target state)
  (let [counts (composition state)
        amount (fn [& kinds] (* mmol (reduce + 0 (map (fn [kind] (get counts kind 0)) kinds))))]
    (equilibrium (amount :hcl :chloride) (amount :ha :anion) (amount :sodium)
                 (+ acid-ml (* drop-ml state.drops)))))

(def (fresh acid ghost)
  (let [state {acid: acid
               particles: (vec (concat (scatter :water water-count) (scatter acid acid-count)))
               ka: ka drops: 0 falling: [] hops: [] log: [] wait: 0 time: 0 ph: 7.0 ghost: ghost}]
    (assoc state :curve [[0 (target state)]])))

(def (restart state acid)
  (fresh acid (if (> (count state.curve) 1)
                {acid: state.acid ka: state.ka curve: state.curve}
                state.ghost)))

(def (bounce at speed low high)
  (cond ((< at low) [(+ low (- low at)) (abs speed)])
    ((> at high) [(- high (- at high)) (- (abs speed))])
    (else [at speed])))

(def (flow x y time)
  "The stirred liquid's velocity at x y: cells of flow whose phase drifts, which mixes it."
  (let [[left top width height] liquid
        across (/ (* 2 math/pi) width)
        down (/ math/pi height)
        phase (* 0.7 time)
        strength (* 70 stirring)]
    [(* strength (math/sin (+ (* across (- x left)) phase)) (math/cos (* down (- y top))))
     (* strength (/ across down) -1 (math/cos (+ (* across (- x left)) phase)) (math/sin (* down (- y top))))]))

(def (jiggle p dt kicks time)
  (let [kick (get kicks p.kind)
        vx (+ (* p.vx 0.96) (between (- kick) kick))
        vy (+ (* p.vy 0.96) (between (- kick) kick))
        [fx fy] (flow p.x p.y time)
        spin (math/clamp (+ (* p.spin 0.98) (between -0.15 0.15)) -2 2)
        [left top width height] liquid
        [x vx] (bounce (+ p.x (* (+ vx fx) dt)) vx (+ left 10) (- (+ left width) 10))
        [y vy] (bounce (+ p.y (* (+ vy fy) dt)) vy (+ top 10) (- (+ top height) 10))]
    (assoc p :x x :y y :vx vx :vy vy :spin spin :angle (+ p.angle (* spin dt)))))

(def (move state dt)
  (let [warmth (math/sqrt (/ temperature 300))
        kicks (into {} (for [[kind about] (entries species)]
                         [kind (* 18 warmth (math/sqrt (/ 18 about.mass)))]))]
    (assoc state :particles (map (fn [p] (jiggle p dt kicks state.time)) state.particles))))

(def (logged log text at)
  (if text (vec (take 4 (cons {text: text at: at} log))) log))

(def (seekers counts)
  "Which kinds move toward which: the scarcer kind of each attracting pair seeks the other."
  (reduce (fn [found [a b]]
            (let [na (get counts a 0)
                  nb (get counts b 0)
                  [seeker partner] (if (<= na nb) [a b] [b a])]
              (if (or (zero? na) (zero? nb))
                found
                (update found seeker (fn [partners] (conj (or partners #{}) partner))))))
          {}
          attractions))

(def (pulled p ps grid wants dt)
  (let [near (filter (fn [j] (contains? wants (get (nth ps j) :kind)))
                     (collide/query grid [(- p.x reach) (- p.y reach) (* 2 reach) (* 2 reach)]))
        gap (fn [j] (let [q (nth ps j)] (+ (abs (- q.x p.x)) (abs (- q.y p.y)))))]
    (if (empty? near)
      p
      (let [q (nth ps (first (sort-by gap near)))
            dx (- q.x p.x)
            dy (- q.y p.y)
            d (max 1 (math/sqrt (+ (* dx dx) (* dy dy))))]
        (assoc p :vx (+ p.vx (* pull dt (/ dx d))) :vy (+ p.vy (* pull dt (/ dy d))))))))

(def (attract ps dt)
  "Ions drawn toward the nearest partner they react with, by charge or hydrogen bonds."
  (let [seeking (seekers (frequencies (map :kind ps)))
        sought (set (apply concat (vals seeking)))
        grid (collide/grid 64)]
    (if (empty? seeking)
      ps
      (do
        (each [i (range (count ps))]
          (let [p (nth ps i)]
            (when (contains? sought p.kind)
              (collide/insert! grid i [p.x p.y 1 1]))))
        (map (fn [p] (if-let [wants (get seeking p.kind)] (pulled p ps grid wants dt) p)) ps)))))
(def (react state dt)
  (let [grid (collide/grid 32)
        rules (into {} (map (fn [rule] [rule.from rule]) (reactions)))]
    (each [i (range (count state.particles))]
      (let [p (nth state.particles i)]
        (collide/insert! grid i [(- p.x 9) (- p.y 9) 18 18])))
    (loop [pairs (collide/pairs grid) ps (attract state.particles dt) used (set []) state state]
      (if (empty? pairs)
        (assoc state :particles ps)
        (let [[i j] (first pairs)
              a (nth ps i)
              b (nth ps j)
              forward (get rules [a.kind b.kind])
              backward (get rules [b.kind a.kind])
              [rule donor acceptor] (cond (forward [forward i j])
                                      (backward [backward j i])
                                      (else [nil nil nil]))]
          (if (and rule
                   (not (contains? used i))
                   (not (contains? used j))
                   (random/chance? rule.chance :from rng))
            (let [[gives takes] rule.to]
              (again (rest pairs)
                     (-> ps
                         (assoc donor (assoc (nth ps donor) :kind gives))
                         (assoc acceptor (assoc (nth ps acceptor) :kind takes)))
                     (conj used i j)
                     (-> state
                         (update :hops conj {from: donor to: acceptor age: 0})
                         (update :log logged (get rule :text) state.time))))
            (again (rest pairs) ps used state)))))))

(def (drip state dt pressed held)
  (let [wait (- state.wait dt)
        room (< (+ state.drops (count state.falling)) most-drops)]
    (cond ((and pressed room) (-> state (update :falling conj 0) (assoc :wait (* 2 drip-every))))
      ((and held room (<= wait 0)) (-> state (update :falling conj 0) (assoc :wait drip-every)))
      (else (assoc state :wait wait)))))

(def (land state)
  (let [[left top width _] liquid
        x (+ left (/ width 2))
        state (-> state
                  (update :particles conj
                          (assoc (particle :hydroxide (- x 6) (+ top 12)) :vx -40 :vy 140)
                          (assoc (particle :sodium (+ x 6) (+ top 12)) :vx 40 :vy 140))
                  (update :drops inc))]
    (update state :curve conj [(* drop-ml state.drops) (target state)])))

(def (fall state dt)
  (let [aged (map (fn [t] (+ t dt)) state.falling)
        landed (count (filter (fn [t] (>= t fall-time)) aged))]
    (loop [n landed
           state (assoc state :falling (remove (fn [t] (>= t fall-time)) aged))]
      (if (zero? n) state (again (dec n) (land state))))))

(def (settle state dt)
  (let [goal (target state)]
    (assoc state
           :time (+ state.time dt)
           :ph (+ state.ph (* (- goal state.ph) (min 1 (* 4 dt))))
           :hops (keep (fn [hop] (when (< hop.age hop-time) (update hop :age + dt))) state.hops))))

(def (step state dt pressed held)
  (-> state (move dt) (react dt) (drip dt pressed held) (fall dt) (settle dt)))

(def indicators
  {universal: [[1 [220 40 40 70]] [3 [235 90 40 70]] [4 [240 150 40 70]] [5 [235 210 50 70]]
               [6 [190 215 60 70]] [7 [70 180 80 70]] [8 [40 160 150 70]] [9 [50 110 220 70]]
               [10 [90 70 200 70]] [12 [140 50 170 70]]]
   phenolphthalein: [[8.2 [255 255 255 14]] [10 [235 50 160 90]]]
   bromothymol-blue: [[6 [235 210 50 70]] [7 [70 170 90 70]] [7.6 [40 90 210 70]]]})

(def (indicator-color ph)
  (let [stops (get indicators indicator)
        [low low-color] (first stops)
        [high high-color] (last stops)]
    (cond ((<= ph low) low-color)
      ((>= ph high) high-color)
      (else (let [[[a ca] [b cb]] (find (fn [[below above]] (<= (first below) ph (first above))) (zip stops (rest stops)))
                  t (/ (- ph a) (- b a))]
              (map (fn [x y] (round (math/lerp x y t))) ca cb))))))

(def (fixed x digits)
  (let [n (round (* (abs x) (math/pow 10 digits)))
        whole (// n (math/pow 10 digits))
        part (- n (* whole (math/pow 10 digits)))]
    (str (if (neg? x) "-" "") whole "." (string/pad-start (str part) digits "0"))))

(def superscripts
  {"-" "⁻" "0" "⁰" "1" "¹" "2" "²" "3" "³" "4" "⁴" "5" "⁵" "6" "⁶" "7" "⁷" "8" "⁸" "9" "⁹"})

(def (scientific x)
  (let [power (int (floor (math/log10 x)))]
    (str (fixed (/ x (math/pow 10 power)) 1) "×10"
         (string/join (map (fn [c] (get superscripts (str c))) (string/chars (str power)))))))

(def acid-names
  {1.8e-5 "acetic acid" 6.8e-4 "hydrofluoric acid" 6.3e-5 "benzoic acid" 4.9e-10 "hydrocyanic acid"})

(def (draw-particle p layer)
  (let [kind (get species p.kind)
        c (math/cos p.angle)
        s (math/sin p.angle)
        water (= p.kind :water)]
    (when-let [halo (get kind :halo)]
      (rodeo/circle [p.x p.y] 14 (conj halo 80) :stroke 2 :layer layer))
    (each [[dx dy r element] kind.atoms]
      (let [x (+ p.x (- (* dx c) (* dy s)))
            y (+ p.y (* dx s) (* dy c))]
        (rodeo/circle [x y] r (conj (get elements element) (if water 150 255)) :layer layer)
        (unless water
          (rodeo/circle [(- x (* r 0.3)) (- y (* r 0.3))] (* r 0.4) [255 255 255 90] :layer layer))))))

(def (draw-beaker state)
  (let [[left top width height] liquid
        x (+ left (/ width 2))
        tint (indicator-color state.ph)
        glass [200 220 240 150]]
    (rodeo/rect [left top width height] tint)
    (rodeo/line [left top] [(+ left width) top] (assoc tint 3 160) :width 2)
    (rodeo/line [(- left 4) (- top 40)] [(- left 4) (+ top height 4)] glass :width 3)
    (rodeo/line [(+ left width 4) (- top 40)] [(+ left width 4) (+ top height 4)] glass :width 3)
    (rodeo/line [(- left 4) (+ top height 4)] [(+ left width 4) (+ top height 4)] glass :width 3)
    (rodeo/rect [(- x 5) 64 10 26] [200 220 240 50])
    (rodeo/rect [(- x 5) 64 10 26] glass :stroke 1)
    (rodeo/text "0.1 M NaOH" [(+ x 12) 70] :size 10 :color [170 190 210])
    (each [t state.falling]
      (rodeo/circle [x (math/lerp 92 top (* (/ t fall-time) (/ t fall-time)))] 3.5 [120 180 255 230] :layer 3))))

(def (draw-hops state)
  (each [hop state.hops]
    (let [from (nth state.particles hop.from)
          to (nth state.particles hop.to)
          t (/ hop.age hop-time)
          x (math/lerp from.x to.x t)
          y (- (math/lerp from.y to.y t) (* 10 (math/sin (* t 3.1416))))]
      (rodeo/circle [x y] 6 [255 255 255 70] :layer 3)
      (rodeo/circle [x y] 3 [255 255 255 255] :layer 3)
      (when (> t 0.6)
        (rodeo/circle [to.x to.y] (+ 10 (* 24 (- t 0.6))) [255 255 255 (round (* 220 (- 1 t)))]
                      :stroke 2 :layer 3)))))

(def (draw-header state)
  (let [weak (= state.acid :ha)
        tint (indicator-color state.ph)]
    (rodeo/text (if weak
                  (str "Weak acid HA: " (get acid-names ka "a weak acid") ", Ka " (scientific ka))
                  "Strong acid: hydrochloric acid, HCl")
                [20 12] :size 15)
    (rodeo/text (str "50 mL of 0.1 M acid, " (fixed (* drop-ml state.drops) 1) " mL of 0.1 M NaOH added")
                [20 36] :size 11 :color [170 180 195])
    (rodeo/circle [452 30] 11 (assoc tint 3 255))
    (rodeo/circle [452 30] 11 [255 255 255 120] :stroke 1)
    (rodeo/text "pH" [470 10] :size 11 :color [170 180 195])
    (rodeo/text (fixed state.ph 2) [490 6] :size 30)))

(def (draw-log state)
  (rodeo/rect [48 120 262 66] [10 14 20 170] :layer 4)
  (each [i (range (count state.log))]
    (let [entry (nth state.log i)
          fade (math/clamp (- 1 (/ (- state.time entry.at) 6)) 0.25 1)]
      (rodeo/text entry.text [56 (+ 126 (* i 14))] :size 10
                  :color [235 235 240 (round (* 255 fade))]))))

(def (draw-legend state)
  (let [counts (composition state)
        shown (if (= state.acid :ha)
                [:ha :anion :hydronium :hydroxide :sodium :water]
                [:hcl :chloride :hydronium :hydroxide :sodium :water])]
    (each [i (range (count shown))]
      (let [kind (nth shown i)
            x (+ 30 (* i 96))]
        (draw-particle {kind: kind x: (+ x 10) y: 404 angle: 0} 4)
        (rodeo/text (str (get-in species [kind :name]) " " (get counts kind 0)) [(+ x 26) 397]
                    :size 11 :color [215 220 230])))))

(def (chart-at [ml ph])
  (let [[x y w h] chart]
    [(+ x (* w (/ ml (* drop-ml most-drops)))) (- (+ y h) (* h (/ ph 14)))]))

(def (dashed from to color)
  (each [i (range 0 24 2)]
    (rodeo/line (map (fn [a b] (math/lerp a b (/ i 24))) from to)
                (map (fn [a b] (math/lerp a b (/ (inc i) 24))) from to)
                color)))

(def (draw-curve points color)
  (each [[a b] (zip points (rest points))]
    (rodeo/line (chart-at a) (chart-at b) color :width 2))
  (each [point points]
    (rodeo/circle (chart-at point) 2.5 color)))

(def curve-colors {hcl: [255 110 90] ha: [255 190 80]})

(def (curve-name acid strength)
  (if (= acid :hcl) "HCl" (str "HA, Ka " (scientific strength))))

(def (draw-chart state)
  (let [[x y w h] chart
        equivalence (* drop-ml acid-count)
        pka (- (math/log10 ka))]
    (rodeo/rect chart [255 255 255 8])
    (rodeo/rect chart [255 255 255 40] :stroke 1)
    (each [ph [0 7 14]]
      (let [[_ at] (chart-at [0 ph])]
        (rodeo/text (str ph) [(- x 22) (- at 6)] :size 10 :color [150 160 175])))
    (each [ml [0 25 50 75]]
      (let [[at _] (chart-at [ml 0])]
        (rodeo/text (str ml) [(- at 6) (+ y h 4)] :size 10 :color [150 160 175])))
    (rodeo/text "mL NaOH" [(+ x w -56) (+ y h -16)] :size 10 :color [150 160 175])
    (rodeo/text "pH" [(- x 46) (+ y (/ h 2) -6)] :size 10 :color [150 160 175])
    (dashed (chart-at [0 7]) (chart-at [80 7]) [255 255 255 40])
    (dashed (chart-at [equivalence 0]) (chart-at [equivalence 14]) [255 255 255 60])
    (rodeo/text "equivalence" [(+ (first (chart-at [equivalence 14])) 4) (+ y 4)] :size 9 :color [150 160 175])
    (when (= state.acid :ha)
      (dashed (chart-at [0 pka]) (chart-at [80 pka]) [255 190 80 90])
      (rodeo/text (str "pKa " (fixed pka 2)) [(+ x 6) (- (second (chart-at [0 pka])) 14)] :size 9
                  :color [255 190 80]))
    (when-let [ghost state.ghost]
      (draw-curve ghost.curve (conj (get curve-colors ghost.acid) 90))
      (rodeo/text (str "earlier: " (curve-name ghost.acid ghost.ka)) [(+ x 8) (+ y 20)] :size 9
                  :color (conj (get curve-colors ghost.acid) 150)))
    (draw-curve state.curve (conj (get curve-colors state.acid) 255))
    (rodeo/text (str "now: " (curve-name state.acid ka)) [(+ x 8) (+ y 6)] :size 9
                :color (get curve-colors state.acid))))

(def (draw-help)
  (rodeo/text "space  add NaOH      s  strong acid      w  weak acid      r  start over"
              [64 618] :size 10 :color [130 140 155]))

(def (start)
  (when (nil? @world)
    (reset! world (fresh :hcl nil))))

(def (tick)
  (start)
  (cond ((rodeo/pressed? :strong) (swap! world restart :hcl))
    ((rodeo/pressed? :weak) (swap! world restart :ha))
    ((rodeo/pressed? :reset) (swap! world (fn [state] (restart state state.acid)))))
  (swap! world step (rodeo/dt) (rodeo/pressed? :drop) (rodeo/held? :drop)))

(def (draw)
  (let [state @world]
    (rodeo/clear [14 18 26])
    (when state
      (draw-beaker state)
      (each [p state.particles]
        (draw-particle p (if (= p.kind :water) 1 2)))
      (draw-hops state)
      (draw-header state)
      (draw-log state)
      (draw-legend state)
      (draw-chart state)
      (draw-help))))
