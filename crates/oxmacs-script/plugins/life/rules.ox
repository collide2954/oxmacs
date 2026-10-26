(module
  (import (ox random))
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
                 (when (= 0 (random/int 5))
                   [(mod index width) (// index width)]))
               (range (* width height)))))

(def (seeded pattern)
  "The live cells a pattern starts with, in the middle of the grid, or a random fifth of the grid for :random."
  (if (= pattern :random)
    (random-cells)
    (cells (map (fn [[x y]] [(+ x 37) (+ y 28)])
                (get patterns pattern (get patterns :acorn))))))

(test "a blinker turns and turns back"
  (let [blinker (cells [[0 1] [1 1] [2 1]])]
    (expect (= (step blinker) (cells [[1 0] [1 1] [1 2]])))
    (expect (= (step (step blinker)) blinker))))

(test "a block stays and a lone cell dies"
  (let [block (cells [[0 0] [1 0] [0 1] [1 1]])]
    (expect (= (step block) block))
    (expect (empty? (step (cells [[0 0]]))))))

(test "a glider moves one cell down and right every four generations"
  (let [glider (cells [[1 0] [2 1] [0 2] [1 2] [2 2]])
        later (reduce (fn [now _] (step now)) glider (range 4))]
    (expect (= later (cells (map (fn [[x y]] [(+ x 1) (+ y 1)]) (points glider)))))))

(test "cells far from the middle live by the same rules"
  (expect (= (points (step (cells [[-5000 -7000] [-4999 -7000] [-4998 -7000]])))
             [[-4999 -7001] [-4999 -7000] [-4999 -6999]])))

(test "patterns start from their cells"
  (expect (= (count (seeded :acorn)) 7))
  (expect (<= (count (seeded :random)) (* width height))))
