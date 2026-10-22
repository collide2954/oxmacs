(def (swap! a f & args)
  (reset! a (apply f @a args)))

(def (update coll k f & args)
  (assoc coll k (apply f (get coll k) args)))
