(deferror error [])
(deferror type-error [])
(deferror arity-error [])
(deferror name-error [])
(deferror range-error [])
(deferror zero-division-error [])
(deferror no-match-error [])
(deferror expectation-failed [])
(def (identity x) "Returns x." x)

(def (constantly x)
  "A function that ignores its arguments and always returns x."
  (fn [& _] x))

(def (not x)
  "True when x is false or nil."
  (if x false true))

(def (nil? x)
  "True when x is nil."
  (identical? x nil))

(def (some? x)
  "True when x is anything but nil."
  (not (nil? x)))

(def (complement f)
  "A function that returns the opposite truth of f: ((complement even?) 3) is true."
  (fn [& args] (not (apply f args))))

(def (comp & fs)
  "A function that runs the functions right to left, each on the result of the next: ((comp inc abs) -3) is 4."
  (let [order (reverse fs)]
    (if (empty? order)
      identity
      (fn [& args]
        ((fn step [left value]
           (if (empty? left) value (step (rest left) ((first left) value))))
         (rest order)
         (apply (first order) args))))))

(def (partial f & fixed)
  "f with its first arguments filled in: ((partial + 10) 5) is 15."
  (fn [& more] (apply f (concat fixed more))))

(def (swap! a f & args)
  "Changes the atom a to (f current args...), retrying if another task changed it meanwhile, and returns the new value."
  (let [old @a
        new (apply f old args)]
    (if (compare-and-set! a old new)
      new
      (apply swap! a f args))))

(def (update coll k f & args)
  "A copy of coll with the value under k changed to (f value args...)."
  (assoc coll k (apply f (get coll k) args)))

(def (map f coll & colls)
  "A vector of f applied to each item of coll, or to the items of several collections side by side until the shortest runs out: (map + [1 2] [10 20]) is [11 22]."
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
  "A vector of the items of coll for which keep? is truthy."
  (let [items (vec coll)
        n (count items)]
    ((fn step [i out]
       (if (< i n)
         (let [item (nth items i)]
           (step (+ i 1) (if (keep? item) (cons item out) out)))
         (reverse out)))
     0 nil)))

(def (remove drop? coll)
  "A vector of the items of coll for which drop? is falsy."
  (filter (complement drop?) coll))

(def (reduce f & args)
  "Combines the items of a collection with f, left to right, starting from init when given: (reduce + 0 [1 2 3]) is 6. Called as (reduce f coll), it starts from the first item."
  (let [items (vec (last args))
        n (count items)
        walk (fn walk [acc i]
               (if (< i n) (walk (f acc (nth items i)) (+ i 1)) acc))]
    (if (= (count args) 1)
      (if (= n 0) (f) (walk (nth items 0) 1))
      (walk (first args) 0))))

(def (some pred coll)
  "The first truthy result of pred on the items of coll, or nil."
  (let [items (vec coll)
        n (count items)]
    ((fn step [i]
       (if (< i n)
         (let [found (pred (nth items i))]
           (if found found (step (+ i 1))))
         nil))
     0)))

(def (every? pred coll)
  "True when pred is truthy for every item of coll."
  (not (some (complement pred) coll)))

(def (keep f coll)
  "A vector of the results of f on the items of coll, leaving out nil."
  (let [items (vec coll)
        n (count items)]
    ((fn step [i out]
       (if (< i n)
         (let [value (f (nth items i))]
           (step (+ i 1) (if (nil? value) out (cons value out))))
         (reverse out)))
     0 nil)))

(def (distinct coll)
  "The items of coll without repeats, in their first order."
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
  "The items of coll sorted by (key item), keeping the order of ties."
  (let [items (vec coll)]
    (map (fn [pair] (nth items (nth pair 1)))
         (sort (map (fn [i] [(key (nth items i)) i]) (range (count items)))))))

(def (mapcat f coll & colls)
  "Maps f over the collections like map and concatenates the results."
  (apply concat (apply map f coll colls)))

(defmacro when [test & body]
  "Evaluates body when test is truthy; otherwise nil."
  `(if ~test (do ~@body) nil))

(defmacro unless [test & body]
  "Evaluates body when test is falsy; otherwise nil."
  `(if ~test nil (do ~@body)))

(defmacro and [& forms]
  "Evaluates forms left to right until one is falsy and returns it, or the last value; (and) is true."
  (if (empty? forms)
    true
    (if (empty? (rest forms))
      (first forms)
      `(let [value ~(first forms)]
         (if value (and ~@(rest forms)) value)))))

(defmacro or [& forms]
  "Evaluates forms left to right until one is truthy and returns it; otherwise the last value, and (or) is nil."
  (if (empty? forms)
    nil
    (if (empty? (rest forms))
      (first forms)
      `(let [value ~(first forms)]
         (if value value (or ~@(rest forms)))))))

(defmacro cond [& clauses]
  "Tries (test body...) clauses in turn and evaluates the body of the first whose test is truthy; an else clause always matches."
  (if (empty? clauses)
    nil
    (let [clause (first clauses)
          test (first clause)
          body (rest clause)]
      (if (= test 'else)
        `(do ~@body)
        `(if ~test (do ~@body) (cond ~@(rest clauses)))))))

(defmacro -> [value & forms]
  "Threads value through the forms as their first argument: (-> x (f 1) g) is (g (f x 1))."
  (reduce (fn [acc form]
            (if (list? form)
              `(~(first form) ~acc ~@(rest form))
              (list form acc)))
          value
          forms))

(defmacro ->> [value & forms]
  "Threads value through the forms as their last argument: (->> xs (map f) (filter g)) is (filter g (map f xs))."
  (reduce (fn [acc form]
            (if (list? form)
              `(~@form ~acc)
              (list form acc)))
          value
          forms))

(defmacro if-let [binding then & otherwise]
  "Like if, binding [name value] for then when value is truthy."
  `(let [value ~(second binding)]
     (if value (let [~(first binding) value] ~then) ~@otherwise)))

(defmacro when-let [binding & body]
  "Like when, binding [name value] for body when value is truthy."
  `(let [value ~(second binding)]
     (if value (let [~(first binding) value] ~@body) nil)))

(defmacro defrule [pattern template]
  "Defines a macro by example: calls that match pattern become template, with ... repeating what it follows."
  `(defmacro ~(first pattern) [& form]
     (expand-rules '~(first pattern) [] '[[~pattern ~template]] form)))

(defmacro defrules [name literals & rules]
  "Defines a macro from [pattern template] rules, using the first that matches; literals are words that match only themselves."
  `(defmacro ~name [& form]
     (expand-rules '~name '~literals '~(vec rules) form)))

(defmacro maybe [form]
  "The value of form, or nil if it raises an error."
  `(try ~form (catch error e nil)))

(defmacro atomically [& body]
  "Evaluates body without letting other tasks run in between."
  `(do (atomic-enter)
     (try (do ~@body) (finally (atomic-leave)))))

(defmacro gen [& body]
  "A generator that runs body, pausing at each yield: (vec (gen (yield 1) (yield 2))) is [1 2]."
  `(generator (fn [] ~@body)))

(defmacro spawn [& body]
  "Runs body as a task of its own and returns a promise of its result."
  `(spawn-call (fn [] ~@body)))

(def (every ms f)
  "Calls f every ms milliseconds in a task of its own, until f returns :stop."
  (spawn ((fn tick [] (sleep ms) (unless (= (f) :stop) (tick))))))

(def (drop n coll)
  "The items of coll after the first n, as a vector."
  (let [items (vec coll)
        total (count items)]
    ((fn step [i out] (if (< i total) (step (+ i 1) (cons (nth items i) out)) (reverse out))) (min n total) nil)))

(def (take-while keep? coll)
  "The items of a collection or generator up to the first for which keep? is falsy."
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
  "The items of coll from the first for which drop? is falsy."
  (let [items (vec coll)
        total (count items)
        start ((fn skip [i] (if (and (< i total) (drop? (nth items i))) (skip (+ i 1)) i)) 0)]
    (drop start items)))

(def (iterate f x)
  "A generator of x, (f x), (f (f x)) and so on, without end: (take 3 (iterate inc 0)) is [0 1 2]."
  (gen ((fn step [value] (yield value) (step (f value))) x)))

(def (chunk n coll)
  "The items of coll in vectors of n, the last one perhaps shorter: (chunk 2 [1 2 3]) is [[1 2] [3]]."
  (let [items (vec coll)
        total (count items)]
    ((fn step [i out]
       (if (< i total)
         (step (+ i n) (conj out (take n (drop i items))))
         out))
     0 [])))

(def (partition n coll)
  "The items of coll in vectors of exactly n, leaving out any short last group."
  (filter (fn [group] (= (count group) n)) (chunk n coll)))

(def (flatten coll)
  "The items of nested vectors and lists, in one flat vector."
  (reduce (fn [out item]
            (if (or (vector? item) (list? item))
              (into out (flatten item))
              (conj out item)))
          [] (vec coll)))

(def (find pred coll)
  "The first item of coll for which pred is truthy, or nil."
  (when-let [found (some (fn [item] (when (pred item) [item])) coll)]
    (first found)))

(def (frequencies coll)
  "A map from each distinct item of coll to how many times it appears."
  (reduce (fn [counts item] (assoc counts item (+ 1 (get counts item 0)))) {} coll))

(def (group-by key coll)
  "A map from each (key item) to a vector of the items that gave it."
  (reduce (fn [groups item]
            (let [k (key item)]
              (assoc groups k (conj (get groups k []) item))))
          {} coll))

(def (zip & colls)
  "Vectors of the items of the collections side by side: (zip [1 2] [:a :b]) is [[1 :a] [2 :b]]."
  (apply map vector colls))

(def (interleave & colls)
  "One item from each collection in turn: (interleave [1 2] [:a :b]) is [1 :a 2 :b]."
  (mapcat identity (apply zip colls)))

(def (repeat n x)
  "A vector of x, n times."
  (map (fn [_] x) (range n)))

(def (get-in m path & default)
  "The value found by following the keys of path into nested maps and vectors, or default, or nil."
  (let [found (reduce (fn [value key] (if (nil? value) nil (get value key))) m path)]
    (if (and (nil? found) (not (empty? default))) (first default) found)))

(def (assoc-in m path value)
  "A copy of nested maps m with value set at the keys of path, making maps on the way as needed."
  (let [key (first path)
        more (rest path)]
    (if (empty? more)
      (assoc m key value)
      (assoc m key (assoc-in (get m key {}) more value)))))

(def (update-in m path f & args)
  "A copy of nested maps m with the value at path changed to (f value args...)."
  (assoc-in m path (apply f (get-in m path) args)))

(def (merge & maps)
  "One map with the entries of all the maps, later ones winning."
  (reduce (fn [out m]
            (if (nil? m)
              out
              (reduce (fn [out key] (assoc out key (get m key))) (or out {}) (keys m))))
          nil maps))

(def (merge-with f & maps)
  "Merges the maps like merge, combining the values of a key found in more than one with f."
  (reduce (fn [out m]
            (if (nil? m)
              out
              (reduce (fn [out key]
                        (assoc out key (if (contains? out key) (f (get out key) (get m key)) (get m key))))
                      (or out {}) (keys m))))
          nil maps))

(def (select-keys m wanted)
  "A map of just the entries of m whose keys are in wanted."
  (reduce (fn [out key] (if (contains? m key) (assoc out key (get m key)) out)) {} wanted))

(def (juxt & fs)
  "A function that calls each of fs with its arguments and returns their results in a vector: ((juxt first last) [1 2 3]) is [1 3]."
  (fn [& args] (vec (map (fn [f] (apply f args)) fs))))

(def (memoize f)
  "f remembering its results, so calling it again with the same arguments returns the first answer."
  (let [seen (atom {})]
    (fn [& args]
      (let [key (vec args)]
        (if (contains? @seen key)
          (get @seen key)
          (let [value (apply f args)]
            (swap! seen assoc key value)
            value))))))

(defmacro some-> [value & forms]
  "Threads value through the forms like ->, stopping with nil as soon as a step gives nil."
  (if (empty? forms)
    value
    (let [form (first forms)
          step (if (list? form) `(~(first form) it ~@(rest form)) (list form 'it))]
      `(let [it ~value]
         (if (nil? it) nil (some-> ~step ~@(rest forms)))))))

(defmacro as-> [value name & forms]
  "Threads value through the forms, binding it to name in each so it can go anywhere: (as-> 5 n (- 10 n) (* n 2)) is 10."
  (if (empty? forms)
    value
    `(let [~name ~value]
       (as-> ~(first forms) ~name ~@(rest forms)))))

(def registered-tests (atom []))

(defmacro test [title & body]
  "Registers a test called title that ox test runs; it passes unless body raises, and expect raises on a false check."
  `(do (swap! registered-tests conj {name: ~title run: (fn [] ~@body)})
     nil))

(def (shown-plainly? form)
  "True when form reads the same as its value, so expect doesn't repeat it."
  (or (number? form) (string? form) (keyword? form) (nil? form) (bool? form) (char? form)))

(def (explain form values)
  "The text expect shows when form failed: the form, then what each argument evaluated to."
  (apply str (pr-str form)
         (map (fn [pair]
                (if (shown-plainly? (nth pair 0))
                  ""
                  "\n  {(pr-str (nth pair 0))} => {(pr-str (nth pair 1))}"))
              (zip (rest form) values))))

(def plain-heads #{"and" "or" "not" "if" "when" "unless" "cond" "let" "do" "match" "try"})

(defmacro expect [form]
  "Checks that form is truthy, raising expectation-failed with what each argument was when it isn't: (expect (= (area 2 3) 6))."
  (if (and (list? form)
           (not (empty? form))
           (symbol? (first form))
           (not (contains? plain-heads (name (first form)))))
    `(let [values (vector ~@(rest form))]
       (if (apply ~(first form) values)
         true
         (raise (expectation-failed (explain '~form values)))))
    `(if ~form true (raise (expectation-failed (pr-str '~form))))))

(def (profile-report found)
  "Prints the :report of a profile."
  (print (get found :report)))
