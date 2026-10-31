(module
  (import (ox math))
  (export background solids edges draw))

(def background "#04050b")
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

(def (seen solid seconds alpha cx cy scale)
  (let [radius (reduce max (map (fn [[x y z]] (math/sqrt (+ (* x x) (* y y) (* z z)))) solid.vertices))
        placed (vec (map (fn [vertex]
                           (let [[x y z] (turned vertex seconds)
                                 far (/ 4.5 (+ 4.5 (/ z radius)))]
                             [(+ cx (* scale far (/ x radius))) (+ cy (* scale far (/ y radius)))]))
                         solid.vertices))]
    (mapcat (fn [[i j]]
              (let [[x1 y1] (nth placed i)
                    [x2 y2] (nth placed j)]
                [[:line x1 y1 x2 y2 [solid.color (* 0.18 alpha)] 7]
                 [:line x1 y1 x2 y2 [solid.color (* 0.95 alpha)] 1.6]]))
            solid.lines)))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        which (int (/ seconds stay))
        along (- seconds (* which stay))
        blend (max 0 (min 1 (- along (- stay 1.5))))
        scale (* 0.36 (min frame.width frame.height))
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        now (nth solids (mod which (count solids)))
        coming (nth solids (mod (inc which) (count solids)))]
    (vec (concat (seen now seconds (- 1 blend) cx cy scale)
                 (if (> blend 0) (seen coming seconds blend cx cy scale) [])))))

(test "the solids have their edges"
  (expect (= (map (fn [solid] (count solid.lines)) solids) [12 12 30 30])))
