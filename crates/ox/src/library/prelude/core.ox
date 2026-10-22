(deferror error [])
(deferror type-error [])
(deferror arity-error [])
(deferror name-error [])
(deferror range-error [])
(deferror zero-division-error [])
(deferror no-match-error [])
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
  (let [old @a
        new (apply f old args)]
    (if (compare-and-set! a old new)
      new
      (apply swap! a f args))))

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

(def (distinct coll)
  (let [items (vec coll)
        n (count items)]
    ((fn step [i seen out]
       (if (< i n)
         (let [item (nth items i)]
           (if (contains? seen item)
             (step (+ i 1) seen out)
             (step (+ i 1) (conj seen item) (conj out item))))
         out))
     0 (set []) [])))

(def (sort-by key coll)
  (let [items (vec coll)]
    (map (fn [pair] (nth items (nth pair 1)))
         (sort (map (fn [i] [(key (nth items i)) i]) (range (count items)))))))

(def (mapcat f coll & colls)
  (apply concat (apply map f coll colls)))

(def vec
  (let [items vec]
    (fn vec [coll]
      (if (generator? coll)
        ((fn drain [out]
           (let [item (next coll)]
             (if (done? coll) out (drain (conj out item)))))
         [])
        (items coll)))))

(defmacro when [test & body]
  `(if ~test (do ~@body) nil))

(defmacro unless [test & body]
  `(if ~test nil (do ~@body)))

(defmacro and [& forms]
  (if (empty? forms)
    true
    (if (empty? (rest forms))
      (first forms)
      `(let [value ~(first forms)]
         (if value (and ~@(rest forms)) value)))))

(defmacro or [& forms]
  (if (empty? forms)
    nil
    (if (empty? (rest forms))
      (first forms)
      `(let [value ~(first forms)]
         (if value value (or ~@(rest forms)))))))

(defmacro cond [& clauses]
  (if (empty? clauses)
    nil
    (let [clause (first clauses)
          test (first clause)
          body (rest clause)]
      (if (= test 'else)
        `(do ~@body)
        `(if ~test (do ~@body) (cond ~@(rest clauses)))))))

(defmacro -> [value & forms]
  (reduce (fn [acc form]
            (if (list? form)
              `(~(first form) ~acc ~@(rest form))
              (list form acc)))
          value
          forms))

(defmacro ->> [value & forms]
  (reduce (fn [acc form]
            (if (list? form)
              `(~@form ~acc)
              (list form acc)))
          value
          forms))

(defmacro if-let [binding then & otherwise]
  `(let [value ~(second binding)]
     (if value (let [~(first binding) value] ~then) ~@otherwise)))

(defmacro when-let [binding & body]
  `(let [value ~(second binding)]
     (if value (let [~(first binding) value] ~@body) nil)))

(defmacro defrule [pattern template]
  `(defmacro ~(first pattern) [& form]
     (expand-rules '~(first pattern) [] '[[~pattern ~template]] form)))

(defmacro defrules [name literals & rules]
  `(defmacro ~name [& form]
     (expand-rules '~name '~literals '~(vec rules) form)))

(defmacro maybe [form]
  `(try ~form (catch error e nil)))

(defmacro gen [& body]
  `(generator (fn [] ~@body)))

(defmacro spawn [& body]
  `(spawn-call (fn [] ~@body)))

(def (every ms f)
  (spawn ((fn tick [] (sleep ms) (unless (= (f) :stop) (tick))))))

(def (take n coll)
  (if (generator? coll)
    ((fn pull [i out]
       (if (< i n)
         (let [item (next coll)]
           (if (done? coll) out (pull (+ i 1) (conj out item))))
         out))
     0 [])
    (let [items (vec coll)
          end (min n (count items))]
      ((fn step [i out] (if (< i end) (step (+ i 1) (conj out (nth items i))) out)) 0 []))))

(def (drop n coll)
  (let [items (vec coll)
        total (count items)]
    ((fn step [i out] (if (< i total) (step (+ i 1) (conj out (nth items i))) out)) (min n total) [])))

(def (take-while keep? coll)
  (if (generator? coll)
    ((fn pull [out]
       (let [item (next coll)]
         (if (or (done? coll) (not (keep? item))) out (pull (conj out item)))))
     [])
    (let [items (vec coll)
          total (count items)]
      ((fn step [i out]
         (if (and (< i total) (keep? (nth items i))) (step (+ i 1) (conj out (nth items i))) out))
       0 []))))

(def (drop-while drop? coll)
  (let [items (vec coll)
        total (count items)
        start ((fn skip [i] (if (and (< i total) (drop? (nth items i))) (skip (+ i 1)) i)) 0)]
    (drop start items)))

(def (iterate f x)
  (gen ((fn step [value] (yield value) (step (f value))) x)))

(def (chunk n coll)
  (let [items (vec coll)
        total (count items)]
    ((fn step [i out]
       (if (< i total)
         (step (+ i n) (conj out (take n (drop i items))))
         out))
     0 [])))

(def (partition n coll)
  (filter (fn [group] (= (count group) n)) (chunk n coll)))

(def (flatten coll)
  (reduce (fn [out item]
            (if (or (vector? item) (list? item))
              (into out (flatten item))
              (conj out item)))
          [] (vec coll)))

(def (find pred coll)
  (when-let [found (some (fn [item] (when (pred item) [item])) coll)]
    (first found)))

(def (frequencies coll)
  (reduce (fn [counts item] (assoc counts item (+ 1 (get counts item 0)))) {} coll))

(def (group-by key coll)
  (reduce (fn [groups item]
            (let [k (key item)]
              (assoc groups k (conj (get groups k []) item))))
          {} coll))

(def (zip & colls)
  (apply map vector colls))

(def (interleave & colls)
  (mapcat identity (apply zip colls)))

(def (repeat n x)
  (map (fn [i] x) (range n)))

(def (get-in m path & default)
  (let [found (reduce (fn [value key] (if (nil? value) nil (get value key))) m path)]
    (if (and (nil? found) (not (empty? default))) (first default) found)))

(def (assoc-in m path value)
  (let [key (first path)
        more (rest path)]
    (if (empty? more)
      (assoc m key value)
      (assoc m key (assoc-in (get m key {}) more value)))))

(def (update-in m path f & args)
  (assoc-in m path (apply f (get-in m path) args)))

(def (merge & maps)
  (reduce (fn [out m]
            (if (nil? m)
              out
              (reduce (fn [out key] (assoc out key (get m key))) (or out {}) (keys m))))
          nil maps))

(def (merge-with f & maps)
  (reduce (fn [out m]
            (if (nil? m)
              out
              (reduce (fn [out key]
                        (assoc out key (if (contains? out key) (f (get out key) (get m key)) (get m key))))
                      (or out {}) (keys m))))
          nil maps))

(def (select-keys m wanted)
  (reduce (fn [out key] (if (contains? m key) (assoc out key (get m key)) out)) {} wanted))

(def (juxt & fs)
  (fn [& args] (vec (map (fn [f] (apply f args)) fs))))

(def (memoize f)
  (let [seen (atom {})]
    (fn [& args]
      (let [key (vec args)]
        (if (contains? @seen key)
          (get @seen key)
          (let [value (apply f args)]
            (swap! seen assoc key value)
            value))))))

(defmacro some-> [value & forms]
  (if (empty? forms)
    value
    (let [form (first forms)
          step (if (list? form) `(~(first form) it ~@(rest form)) (list form 'it))]
      `(let [it ~value]
         (if (nil? it) nil (some-> ~step ~@(rest forms)))))))

(defmacro as-> [value name & forms]
  (if (empty? forms)
    value
    `(let [~name ~value]
       (as-> ~(first forms) ~name ~@(rest forms)))))
