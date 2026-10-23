(module (import (ox string)))

(def n 100000)
(def numbers (vec (range n)))
(def point {x: 1 y: 2 z: 3})

(def (count-up i) (if (< i n) (count-up (+ i 1)) i))
(def (ident x) x)
(def (calls i) (if (< i n) (do (ident i) (calls (+ i 1))) i))
(def (conjs i out) (if (< i n) (conjs (+ i 1) (conj out i)) out))
(def (literals i) (if (< i n) (do [i i i i i] (literals (+ i 1))) i))
(def (fields i) (if (< i n) (do point.x (fields (+ i 1))) i))
(def (hashes i) (if (< i n) (do (hash [i 1]) (hashes (+ i 1))) i))
(def (qualified i) (if (< i n) (do (string/blank? "a") (qualified (+ i 1))) i))
(def (floats i x) (if (< i n) (floats (+ i 1) (* x 1.0000001)) x))
(def (closures i) (if (< i n) (do (fn [] i) (closures (+ i 1))) i))

{loop: [n (fn [] (count-up 0))]
 call: [n (fn [] (calls 0))]
 conj: [n (fn [] (conjs 0 []))]
 vector: [n (fn [] (literals 0))]
 field: [n (fn [] (fields 0))]
 hash: [n (fn [] (hashes 0))]
 qualified: [n (fn [] (qualified 0))]
 float: [n (fn [] (floats 0 1.0))]
 closure: [n (fn [] (closures 0))]
 map: [n (fn [] (map inc numbers))]
 keep: [n (fn [] (keep (fn [x] (when (even? x) x)) numbers))]
 mapcat: [n (fn [] (mapcat (fn [x] [x x]) numbers))]
 reduce: [n (fn [] (reduce + 0 numbers))]
 assoc: [n (fn [] (reduce (fn [acc i] (assoc point :x i)) 0 numbers))]}
