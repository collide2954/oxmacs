(module
  (import (ox math) (screensaver plasma :as plasma))
  (export background escape draw))

(def background "#000000")
(def tile 12)
(def target [-0.743643887037151 0.13182590420533])
(def dive 50.0)

(def (escape cx cy most)
  "How many steps z*z+c takes to leave the circle of radius 2 from 0, or nil if it stays."
  (loop [x 0.0 y 0.0 k 0]
    (let [xx (* x x) yy (* y y)]
      (cond
        [(> (+ xx yy) 4) k]
        [(>= k most) nil]
        [else (again (+ (- xx yy) cx) (+ (* 2 x y) cy) (inc k))]))))

(defonce screen (atom {key: nil steps: {} turn: 0}))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        along (mod seconds dive)
        scale (* 0.025 (math/exp (* -0.1 along)))
        most (+ 60 (int (* 4 along)))
        columns (+ 1 (int (/ frame.width tile)))
        rows (+ 1 (int (/ frame.height tile)))
        known @screen
        key [columns rows (int (/ seconds dive))]
        state (if (= known.key key) known {key: key steps: {} turn: 0})
        [tx ty] target
        per (/ scale frame.width)
        cycle (int (* 12 seconds))
        fresh (reduce (fn [found index]
                        (let [x (+ tx (* per (- (* tile (mod index columns)) (/ frame.width 2))))
                              y (+ ty (* per (- (* tile (int (/ index columns))) (/ frame.height 2))))]
                          (assoc found index (escape x y most))))
                      state.steps
                      (filter (fn [index] (= (mod index 3) (mod state.turn 3))) (range (* columns rows))))]
    (reset! screen {key: key steps: fresh turn: (inc state.turn)})
    (vec (keep (fn [index]
                 (when-let [steps (get fresh index)]
                   [:rect (* tile (mod index columns)) (* tile (int (/ index columns))) tile tile
                    (nth plasma/palette (mod (+ (int (* 18 (math/log (inc steps)))) cycle) 64))]))
               (keys fresh)))))

(test "points in the set stay and others leave"
  (expect (nil? (escape 0 0 100)))
  (expect (nil? (escape -1 0 100)))
  (expect (= (escape 1 1 100) 2))
  (expect (number? (escape 0.4 0.4 200))))
