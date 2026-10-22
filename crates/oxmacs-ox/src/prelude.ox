(defmacro defcommand [name summary & body]
  `(define-command '~name ~summary (fn [] ~@body)))
