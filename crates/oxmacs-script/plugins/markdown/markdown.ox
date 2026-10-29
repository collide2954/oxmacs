(module
  (import (ox string)
          (markdown grammar :as grammar)
          (markdown preview :as preview)
          (markdown style :as style)))

(def (items)
  "The headings of the current buffer, indented by level, for M-g i."
  (vec (map (fn [found] {label: (str (string/repeat "  " (- found.level 1)) found.title)
                         note: "L{(+ found.line 1)}"
                         line: found.line})
            (grammar/headings (buffer-text)))))

(defmode markdown "Markdown"
  :highlight style/highlight
  :items items)

(define-language-server "marksman"
  :languages {"Markdown" "markdown"}
  :program "marksman"
  :args ["server"]
  :install "download marksman from https://github.com/artempyanykh/marksman/releases")

(defcommand markdown-preview "Show this Markdown buffer rendered beside it"
  (preview/show))
