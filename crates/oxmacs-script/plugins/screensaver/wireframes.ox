(module
  (import (ox math))
  (export solids edges draw))
(def golden (/ (+ 1 (math/sqrt 5)) 2))
(def stay 7.0)

(def (signs)
  [1 -1])

(def (edges vertices)
  "Pairs of vertices as far apart as the nearest two, which for these solids are their edges."
  (let [apart (fn [[ax ay az] [bx by bz]]
                (let [dx (- ax bx) dy (- ay by) dz (- az bz)] (+ (* dx dx) (* dy dy) (* dz dz))))
        pairs (mapcat (fn [i] (map (fn [j] [i j]) (range (inc i) (count vertices)))) (range (count vertices)))
        nearest (reduce min (map (fn [[i j]] (apart (nth vertices i) (nth vertices j))) pairs))]
    (vec (filter (fn [[i j]] (< (abs (- (apart (nth vertices i) (nth vertices j)) nearest)) 0.001)) pairs))))

(def cube
  (vec (for [x (signs) y (signs) z (signs)] [x y z])))

(def octahedron
  [[1.4 0 0] [-1.4 0 0] [0 1.4 0] [0 -1.4 0] [0 0 1.4] [0 0 -1.4]])

(def icosahedron
  (vec (mapcat (fn [[a b]] [[0 a (* b golden)] [a (* b golden) 0] [(* b golden) 0 a]])
               (for [a (signs) b (signs)] [a b]))))

(def dodecahedron
  (let [small (/ 1 golden)]
    (vec (concat cube
                 (mapcat (fn [[a b]] [[0 (* a small) (* b golden)] [(* a small) (* b golden) 0] [(* b golden) 0 (* a small)]])
                         (for [a (signs) b (signs)] [a b]))))))

(def solids
  (vec (map (fn [[vertices color]] {vertices: vertices lines: (edges vertices) color: color})
            [[cube "#4cc9f0"] [octahedron "#f72585"] [icosahedron "#ffd166"] [dodecahedron "#06d6a0"]])))

(def (turned [x y z] seconds)
  (let [a (* 0.47 seconds) b (* 0.31 seconds) c (* 0.19 seconds)
        y1 (- (* y (math/cos a)) (* z (math/sin a)))
        z1 (+ (* y (math/sin a)) (* z (math/cos a)))
        x2 (+ (* x (math/cos b)) (* z1 (math/sin b)))
        z2 (- (* z1 (math/cos b)) (* x (math/sin b)))
        x3 (- (* x2 (math/cos c)) (* y1 (math/sin c)))
        y3 (+ (* x2 (math/sin c)) (* y1 (math/cos c)))]
    [x3 y3 z2]))

(def (seen solid seconds alpha cx cy scale floor)
  (let [radius (reduce max (map (fn [[x y z]] (math/sqrt (+ (* x x) (* y y) (* z z)))) solid.vertices))
        placed (vec (map (fn [vertex]
                           (let [[x y z] (turned vertex seconds)
                                 far (/ 4.5 (+ 4.5 (/ z radius)))]
                             [(+ cx (* scale far (/ x radius))) (+ cy (* scale far (/ y radius))) (/ z radius)]))
                         solid.vertices))
        edge (fn [mirror strength]
               (mapcat (fn [[i j]]
                         (let [[x1 y1 z1] (nth placed i)
                               [x2 y2 z2] (nth placed j)
                               near (- 1 (* 0.35 (+ 1 (/ (+ z1 z2) 2))))
                               a (* alpha strength (+ 0.35 (* 0.65 near)))
                               y1 (if mirror (- (* 2 floor) y1) y1)
                               y2 (if mirror (- (* 2 floor) y2) y2)]
                           [[:line x1 y1 x2 y2 [solid.color (* 0.10 a)] 9]
                            [:line x1 y1 x2 y2 [solid.color (* 0.25 a)] 4]
                            [:line x1 y1 x2 y2 [(if mirror solid.color "#f4f8ff") (* 0.9 a)] (+ 0.8 (* 0.9 near))]]))
                       solid.lines))
        corners (fn [mirror strength]
                  (map (fn [[x y z]]
                         [:glow x (if mirror (- (* 2 floor) y) y) (+ 6 (* 5 (- 1 z))) [solid.color (* 0.6 alpha strength)]])
                       placed))]
    (concat (edge true 0.18) (corners true 0.15) (edge false 1.0) (corners false 1.0))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        which (int (/ seconds stay))
        along (- seconds (* which stay))
        blend (max 0 (min 1 (- along (- stay 1.5))))
        scale (* 0.30 (min frame.width frame.height))
        cx (/ frame.width 2.0)
        cy (* 0.44 frame.height)
        floor (* 0.80 frame.height)
        now (nth solids (mod which (count solids)))
        coming (nth solids (mod (inc which) (count solids)))]
    (vec (concat [[:glow cx floor (* 1.2 scale) ["#9ab6ff" 0.06]]]
                 (seen now seconds (- 1 blend) cx cy scale floor)
                 (if (> blend 0) (seen coming seconds blend cx cy scale floor) [])))))

(test "the solids have their edges"
  (expect (= (map (fn [solid] (count solid.lines)) solids) [12 12 30 30])))
