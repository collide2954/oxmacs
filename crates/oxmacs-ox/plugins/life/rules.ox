(module
  (import (ox crypto))
  (export width height step seeded cells points))

(def width 80)
(def height 60)

(def stride 2097152)
(def offset 1048576)

(def patterns
  {acorn: [[1 0] [3 1] [0 2] [1 2] [4 2] [5 2] [6 2]]
   r-pentomino: [[1 0] [2 0] [0 1] [1 1] [1 2]]
   glider: [[1 0] [2 1] [0 2] [1 2] [2 2]]})

(def around
  [(- -1 stride) (- stride) (- 1 stride) -1 1 (- stride 1) stride (+ stride 1)])

(def (cells points)
  "Live cells from [x y] points, as a sorted vector of whole numbers."
  (sort (vec (set (map (fn [[x y]] (+ (* (+ y offset) stride) x offset)) points)))))

(def (points cells)
  "The [x y] points of live cells."
  (map (fn [cell] [(- (mod cell stride) offset) (- (// cell stride) offset)]) cells))

(def (run-end near n cell e)
  (if (and (< e n) (= (nth near e) cell)) (run-end near n cell (+ e 1)) e))

(def (skip-to cells m cell j)
  (if (and (< j m) (< (nth cells j) cell)) (skip-to cells m cell (+ j 1)) j))

(def (walk near n cells m i j out)
  (if (>= i n)
    out
    (let [cell (nth near i)
          end (run-end near n cell (+ i 1))
          neighbors (- end i)
          j (skip-to cells m cell j)
          alive (and (< j m) (= (nth cells j) cell))]
      (walk near n cells m end j (if (or (= neighbors 3) (and (= neighbors 2) alive)) (conj out cell) out)))))

(def (step cells)
  "The next generation of live cells: a cell lives on with two or three neighbors, and one is born with three."
  (let [near (sort (apply concat (map (fn [d] (map (fn [cell] (+ cell d)) cells)) around)))]
    (walk near (count near) cells (count cells) 0 0 [])))

(def (random-cells)
  (cells (keep (fn [index]
                 (when (= 0 (crypto/random-below 5))
                   [(mod index width) (// index width)]))
               (range (* width height)))))

(def (seeded pattern)
  "The live cells a pattern starts with, in the middle of the grid, or a random fifth of the grid for :random."
  (if (= pattern :random)
    (random-cells)
    (cells (map (fn [[x y]] [(+ x 37) (+ y 28)])
                (get patterns pattern (get patterns :acorn))))))
