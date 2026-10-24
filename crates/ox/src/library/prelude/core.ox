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

(defmacro test [title & body]
  "Registers a test called title that ox test runs; it passes unless body raises, and expect raises on a false check."
  `(do (swap! registered-tests conj {name: ~title run: (fn [] ~@body)})
     nil))

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
