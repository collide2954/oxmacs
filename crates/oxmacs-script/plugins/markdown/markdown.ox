(module
  (import (markdown preview :as preview)))

(defcommand markdown-preview "Show this Markdown buffer rendered beside it"
  (preview/show))
