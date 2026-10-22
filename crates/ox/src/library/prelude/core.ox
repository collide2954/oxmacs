(deferror error [])
(deferror type-error [])
(deferror arity-error [])
(deferror name-error [])
(deferror range-error [])
(deferror zero-division-error [])
(deferror no-match-error [])
(deferror expectation-failed [])
(def (identity x) x)

(def (constantly x)
  (fn [& _] x))

(def (not x)
  (if x false true))

(def (nil? x)
  (identical? x nil))

(def (some? x)
  (not (nil? x)))

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
         (if (< i n) (step (+ i 1) (cons (f (nth items i)) out)) (reverse out)))
       0 nil))
    (let [all (reduce (fn [vectors c] (conj vectors (vec c))) [] (cons coll colls))
          n (apply min (reduce (fn [counts c] (conj counts (count c))) [] all))]
      ((fn step [i out]
         (if (< i n)
           (step (+ i 1) (cons (apply f (reduce (fn [items c] (conj items (nth c i))) [] all)) out))
           (reverse out)))
       0 nil))))

(def (filter keep? coll)
  (let [items (vec coll)
        n (count items)]
    ((fn step [i out]
       (if (< i n)
         (let [item (nth items i)]
           (step (+ i 1) (if (keep? item) (cons item out) out)))
         (reverse out)))
     0 nil)))

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
           (step (+ i 1) (if (nil? value) out (cons value out))))
         (reverse out)))
     0 nil)))

(def (distinct coll)
  (let [items (vec coll)
        n (count items)]
    ((fn step [i seen out]
       (if (< i n)
         (let [item (nth items i)]
           (if (contains? seen item)
             (step (+ i 1) seen out)
             (step (+ i 1) (conj seen item) (cons item out))))
         (reverse out)))
     0 (set []) nil)))

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
             (if (done? coll) (reverse out) (drain (cons item out)))))
         nil)
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

(defmacro atomically [& body]
  `(do (atomic-enter)
       (try (do ~@body) (finally (atomic-leave)))))

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
           (if (done? coll) (reverse out) (pull (+ i 1) (cons item out))))
         (reverse out)))
     0 nil)
    (let [items (vec coll)
          end (min n (count items))]
      ((fn step [i out] (if (< i end) (step (+ i 1) (cons (nth items i) out)) (reverse out))) 0 nil))))

(def (drop n coll)
  (let [items (vec coll)
        total (count items)]
    ((fn step [i out] (if (< i total) (step (+ i 1) (cons (nth items i) out)) (reverse out))) (min n total) nil)))

(def (take-while keep? coll)
  (if (generator? coll)
    ((fn pull [out]
       (let [item (next coll)]
         (if (or (done? coll) (not (keep? item))) (reverse out) (pull (cons item out)))))
     nil)
    (let [items (vec coll)
          total (count items)]
      ((fn step [i out]
         (if (and (< i total) (keep? (nth items i))) (step (+ i 1) (cons (nth items i) out)) (reverse out)))
       0 nil))))

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

(def registered-tests (atom []))

(defmacro test [title & body]
  `(do (swap! registered-tests conj {name: ~title run: (fn [] ~@body)})
       nil))

(def (shown-plainly? form)
  (or (number? form) (string? form) (keyword? form) (nil? form) (bool? form) (char? form)))

(def (explain form values)
  (apply str (pr-str form)
         (map (fn [pair]
                (if (shown-plainly? (nth pair 0))
                  ""
                  "\n  {(pr-str (nth pair 0))} => {(pr-str (nth pair 1))}"))
              (zip (rest form) values))))

(def plain-heads #{"and" "or" "not" "if" "when" "unless" "cond" "let" "do" "match" "try"})

(defmacro expect [form]
  (if (and (list? form)
           (not (empty? form))
           (symbol? (first form))
           (not (contains? plain-heads (name (first form)))))
    `(let [values (vector ~@(rest form))]
       (if (apply ~(first form) values)
         true
         (raise (expectation-failed (explain '~form values)))))
    `(if ~form true (raise (expectation-failed (pr-str '~form))))))

(def (profile work)
  (let [mine (profiler-start)]
    (try (work) (catch error e (when mine (profiler-stop)) (raise e)))
    (if mine (profiler-stop) (profiler-snapshot))))

(def (profile-report found)
  (print (get found :report)))
