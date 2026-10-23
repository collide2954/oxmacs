(defmacro defcommand [name summary & body]
  `(define-command '~name ~summary (fn [] ~@body)))

(defmacro defmode [name title & options]
  `(define-mode '~name ~title ~@options))

(defmacro plugin [name & options]
  (list 'quote (cons name options)))

(defmacro use-plugin [name & options]
  `(enable-plugin '~name ~@options))

(def (setting _key)
  (raise (error "setting reads a plugin's own settings, so it only works in a plugin's entry file")))
