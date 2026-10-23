(module
  (import (ox crypto))
  (export width height step seeded))

(def width 80)
(def height 60)

(def patterns
  {acorn: [[1 0] [3 1] [0 2] [1 2] [4 2] [5 2] [6 2]]
   r-pentomino: [[1 0] [2 0] [0 1] [1 1] [1 2]]
   glider: [[1 0] [2 1] [0 2] [1 2] [2 2]]})

(def offsets [[-1 -1] [0 -1] [1 -1] [-1 0] [1 0] [-1 1] [0 1] [1 1]])

(def (neighbors [x y])
  (map (fn [[dx dy]] [(+ x dx) (+ y dy)]) offsets))

(def (step cells)
  "The next generation of a set of live cells: a cell lives on with two or three neighbors, and one is born with three."
  (let [counts (frequencies (mapcat neighbors cells))]
    (set (keep (fn [[cell n]]
                 (when (or (= n 3) (and (= n 2) (contains? cells cell)))
                   cell))
               (entries counts)))))

(def (random-cells)
  (set (keep (fn [index]
               (when (= 0 (crypto/random-below 5))
                 [(mod index width) (// index width)]))
             (range (* width height)))))

(def (seeded pattern)
  "The live cells a pattern starts with, in the middle of the grid, or a random fifth of the grid for :random."
  (if (= pattern :random)
    (random-cells)
    (set (map (fn [[x y]] [(+ x 37) (+ y 28)])
              (get patterns pattern (get patterns :acorn))))))
