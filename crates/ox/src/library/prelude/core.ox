(def (identity x) x)

(def (constantly x)
  (fn [& _] x))

(def (not x)
  (if x false true))

(def (nil? x)
  (identical? x nil))

(def (some? x)
  (not (nil? x)))

(def (inc n)
  (+ n 1))

(def (dec n)
  (- n 1))

(def (complement f)
  (fn [& args] (not (apply f args))))

(def (comp & fs)
  (let [order (reverse fs)]
    (if (empty? order)
      identity
      (fn [& args]
        ((fn step [left value]
           (if (empty? left) value (step (rest left) ((first left) value))))
         (rest order)
         (apply (first order) args))))))

(def (partial f & fixed)
  (fn [& more] (apply f (concat fixed more))))

(def (swap! a f & args)
  (reset! a (apply f @a args)))

(def (update coll k f & args)
  (assoc coll k (apply f (get coll k) args)))

(def (map f coll & colls)
  (if (empty? colls)
    (let [items (vec coll)
          n (count items)]
      ((fn step [i out]
         (if (< i n) (step (+ i 1) (conj out (f (nth items i)))) out))
       0 []))
    (let [all (reduce (fn [vectors c] (conj vectors (vec c))) [] (cons coll colls))
          n (apply min (reduce (fn [counts c] (conj counts (count c))) [] all))]
      ((fn step [i out]
         (if (< i n)
           (step (+ i 1) (conj out (apply f (reduce (fn [items c] (conj items (nth c i))) [] all))))
           out))
       0 []))))

(def (filter keep? coll)
  (let [items (vec coll)
        n (count items)]
    ((fn step [i out]
       (if (< i n)
         (let [item (nth items i)]
           (step (+ i 1) (if (keep? item) (conj out item) out)))
         out))
     0 [])))

(def (remove drop? coll)
  (filter (complement drop?) coll))

(def (reduce f & args)
  (let [items (vec (last args))
        n (count items)
        walk (fn walk [acc i]
               (if (< i n) (walk (f acc (nth items i)) (+ i 1)) acc))]
    (if (= (count args) 1)
      (if (= n 0) (f) (walk (nth items 0) 1))
      (walk (first args) 0))))

(def (some pred coll)
  (let [items (vec coll)
        n (count items)]
    ((fn step [i]
       (if (< i n)
         (let [found (pred (nth items i))]
           (if found found (step (+ i 1))))
         nil))
     0)))

(def (every? pred coll)
  (not (some (complement pred) coll)))

(def (keep f coll)
  (let [items (vec coll)
        n (count items)]
    ((fn step [i out]
       (if (< i n)
         (let [value (f (nth items i))]
           (step (+ i 1) (if (nil? value) out (conj out value))))
         out))
     0 [])))

(def (mapcat f coll & colls)
  (apply concat (apply map f coll colls)))
