; Adapted from tree-sitter-markdown-inline's highlights.scm (MIT).
; When patterns capture the same text, the later one wins.

(emphasis) @constant

(strong_emphasis) @keyword

(code_span) @string

[
  (link_destination)
  (uri_autolink)
] @comment

[
  (link_text)
  (link_label)
  (image_description)
] @link
