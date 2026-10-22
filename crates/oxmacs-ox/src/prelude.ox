(defmacro defcommand [name summary & body]
  `(define-command '~name ~summary (fn [] ~@body)))

(defmacro plugin [name & options]
  (list 'quote (cons name options)))

(defmacro use-plugin [name & options]
  `(enable-plugin '~name ~@options))
