(module
  (import (screensaver noise :as noise))
  (export background algorithms shuffled replay draw))

(def background "#0c0d12")
(def bars 64)
(def playing 9.0)
(def resting 1.5)

(def (swap values i j)
  (assoc values i (nth values j) j (nth values i)))

(def (bubble values)
  "The swaps bubble sort makes, one pass after another."
  (loop [v values pass 0 i 0 out []]
    (cond
      [(>= pass (dec bars)) out]
      [(>= i (- bars 1 pass)) (again v (inc pass) 0 out)]
      [(> (nth v i) (nth v (inc i))) (again (swap v i (inc i)) pass (inc i) (conj out [i (inc i)]))]
      [else (again v pass (inc i) (conj out [i i]))])))

(def (insertion values)
  "The swaps insertion sort makes, each one moving a bar down into place."
  (loop [v values i 1 j 1 out []]
    (cond
      [(>= i bars) out]
      [(and (> j 0) (> (nth v (dec j)) (nth v j))) (again (swap v j (dec j)) i (dec j) (conj out [(dec j) j]))]
      [else (again v (inc i) (inc i) out)])))

(def (selection values)
  "The swaps selection sort makes after looking for each smallest bar."
  (loop [v values i 0 out []]
    (if (>= i (dec bars))
      out
      (let [least (reduce (fn [best k] (if (< (nth v k) (nth v best)) k best)) i (range (inc i) bars))
            looked (vec (map (fn [k] [k k]) (range i bars)))]
        (again (swap v i least) (inc i) (vec (concat out looked [[i least]])))))))

(def (shell values)
  "The swaps Shell sort makes, insertion sorting with shrinking gaps."
  (loop [v values gaps [40 13 4 1] out []]
    (if (empty? gaps)
      out
      (let [gap (first gaps)
            [sorted made] (reduce (fn [[w ops] i]
                                    (loop [w w j i ops ops]
                                      (if (and (>= j gap) (> (nth w (- j gap)) (nth w j)))
                                        (again (swap w j (- j gap)) (- j gap) (conj ops [(- j gap) j]))
                                        [w ops])))
                                  [v out] (range gap bars))]
        (again sorted (rest gaps) made)))))

(def (quick values)
  "The swaps quicksort makes, partitioning around the last bar of each range."
  (loop [v values ranges [[0 (dec bars)]] out []]
    (if (empty? ranges)
      out
      (let [[low high] (last ranges)
            others (vec (take (dec (count ranges)) ranges))]
        (if (>= low high)
          (again v others out)
          (let [pivot (nth v high)
                [w at ops] (reduce (fn [[w at ops] k]
                                     (if (< (nth w k) pivot)
                                       [(swap w at k) (inc at) (conj ops [at k])]
                                       [w at (conj ops [k k])]))
                                   [v low out] (range low high))
                placed (swap w at high)]
            (again placed (conj (conj others [low (dec at)]) [(inc at) high]) (conj ops [at high]))))))))

(def algorithms
  [{name: "Bubble sort" sort: bubble}
   {name: "Insertion sort" sort: insertion}
   {name: "Selection sort" sort: selection}
   {name: "Shell sort" sort: shell}
   {name: "Quicksort" sort: quick}])

(def (shuffled round)
  (vec (map last (sort-by first (map (fn [k] [(noise/of [round k]) (inc k)]) (range bars))))))

(defonce plays (atom {}))

(def (replay round)
  "The bars of round and the swaps its algorithm makes to sort them, worked out once."
  (or (get @plays round)
      (let [values (shuffled round)
            algorithm (nth algorithms (mod round (count algorithms)))
            found {name: algorithm.name values: values swaps: ((get algorithm :sort) values)}]
        (reset! plays {round found})
        found)))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        span (+ playing resting)
        round (int (/ seconds span))
        along (- seconds (* round span))
        play (replay round)
        done (int (* (count play.swaps) (min 1 (/ along playing))))
        values (reduce (fn [v [i j]] (swap v i j)) play.values (take done play.swaps))
        [hi hj] (if (and (> done 0) (< done (count play.swaps))) (nth play.swaps (dec done)) [-1 -1])
        wide (/ frame.width bars)
        tall (* 0.8 frame.height)]
    (vec (concat
          (map (fn [k]
                 (let [h (* tall (/ (nth values k) bars))
                       color (cond
                               [(or (= k hi) (= k hj)) "#ff6b6b"]
                               [(>= along playing) "#06d6a0"]
                               [else "#8ecae6"])]
                   [:rect (+ (* k wide) 1) (- frame.height h) (- wide 2) h color]))
               (range bars))
          [[:text 24 20 play.name :foreground (* 0.045 frame.height)]
           [:text 24 (+ 24 (* 0.06 frame.height)) $"{done} of {(count play.swaps)} steps" :comment (* 0.028 frame.height)]]))))

(test "every algorithm sorts the bars"
  (let [values (shuffled 3)
        sorted (vec (range 1 (inc bars)))]
    (expect (every? (fn [algorithm]
                      (= (reduce (fn [v [i j]] (swap v i j)) values ((get algorithm :sort) values)) sorted))
                    algorithms))))
