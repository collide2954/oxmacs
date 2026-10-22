(module (export of))

(def (of key)
  (/ (mod (hash key) 1000003) 1000003.0))