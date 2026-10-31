(module
  (import (screensaver noise :as noise))
  (export background walk draw))

(def background "#0e0e12")
(def cell 9)
(def pace 1500)
(def most 2500)
(def colors ["#ff6b6b" "#4ecdc4" "#ffe66d"])
(def turns [[0 -1] [1 0] [0 1] [-1 0]])

(defonce world (atom nil))

(def (fresh columns rows)
  {columns: columns rows: rows steps: 0 cells: {}
   ants: (vec (map (fn [k]
                     {x: (int (* columns (+ 0.3 (* 0.4 (noise/of [k :x])))))
                      y: (int (* rows (+ 0.3 (* 0.4 (noise/of [k :y])))))
                      facing: (mod k 4) id: k})
                   (range (count colors))))})

(def (step now)
  "One move of every ant: on an empty cell turn right, on a filled one turn left, flip the cell and go on."
  (reduce (fn [state ant]
            (let [key (+ ant.x (* ant.y state.columns))
                  filled (contains? state.cells key)
                  facing (mod (+ ant.facing (if filled 3 1)) 4)
                  [dx dy] (nth turns facing)
                  cells (if filled (dissoc state.cells key) (assoc state.cells key ant.id))
                  moved {x: (mod (+ ant.x dx) state.columns) y: (mod (+ ant.y dy) state.rows) facing: facing id: ant.id}]
              (assoc state :cells cells :ants (assoc state.ants ant.id moved))))
          now
          now.ants))

(def (walk state wanted)
  "state after its ants have taken wanted steps in all, at most most of them now."
  (loop [now state left (min most (- wanted state.steps))]
    (if (<= left 0)
      now
      (again (assoc (step now) :steps (inc now.steps)) (dec left)))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        columns (int (/ frame.width cell))
        rows (int (/ frame.height cell))
        known @world
        state (if (or (nil? known) (not= known.columns columns) (> known.steps (* pace 80)) (< (* pace seconds) known.steps))
                (fresh columns rows)
                known)
        moved (walk state (int (* pace seconds)))]
    (reset! world moved)
    (vec (concat (map (fn [key]
                        [:rect (* cell (mod key columns)) (* cell (int (/ key columns))) (- cell 1) (- cell 1)
                         [(nth colors (get moved.cells key)) 0.85]])
                      (keys moved.cells))
                 (map (fn [ant] [:rect (- (* cell ant.x) 1) (- (* cell ant.y) 1) (+ cell 1) (+ cell 1) "#ffffff"])
                      moved.ants)))))

(test "an ant walks a square out of its first four steps"
  (let [start (assoc (fresh 20 20) :ants [{x: 10 y: 10 facing: 0 id: 0}])
        after (walk start 4)]
    (expect (= (count after.cells) 4))
    (expect (= [(get (first after.ants) :x) (get (first after.ants) :y)] [10 10]))))
