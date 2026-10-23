(defmacro defcommand [name summary & body]
  "Defines an M-x command called name, described by summary, that runs body."
  `(define-command '~name ~summary (fn [] ~@body)))

(defmacro defmode [name title & options]
  "Defines a major mode; see define-mode for its options."
  `(define-mode '~name ~title ~@options))

(defmacro plugin [name & options]
  "The manifest in a plugin's plugin.ox: its :version, :description, :entry file, :commands, :modes and :settings."
  (list 'quote (cons name options)))

(defmacro use-plugin [name & options]
  "Turns on the plugin called name, with :config for its settings or :disabled true."
  `(enable-plugin '~name ~@options))

(def (setting _key)
  "Reads one of the plugin's own settings; only in a plugin's entry file."
  (raise (error "setting reads a plugin's own settings, so it only works in a plugin's entry file")))
