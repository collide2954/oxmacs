(module
  (import (markdown preview :as preview)
          (markdown style :as style)))

(defmode markdown "Markdown"
  :highlight style/highlight)

(define-language-server "marksman"
  :languages {"Markdown" "markdown"}
  :program "marksman"
  :args ["server"]
  :install "download marksman from https://github.com/artempyanykh/marksman/releases")

(defcommand markdown-preview "Show this Markdown buffer rendered beside it"
  (preview/show))
