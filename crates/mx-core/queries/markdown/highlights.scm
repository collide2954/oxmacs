; Adapted from tree-sitter-markdown's highlights.scm (MIT).
; When patterns capture the same text, the later one wins.

(block_quote) @comment.documentation

[
  (indented_code_block)
  (code_fence_content)
] @string

[
  (fenced_code_block_delimiter)
  (info_string)
  (thematic_break)
  (minus_metadata)
  (plus_metadata)
] @comment

(html_block) @attribute

[
  (list_marker_plus)
  (list_marker_minus)
  (list_marker_star)
  (list_marker_dot)
  (list_marker_parenthesis)
  (task_list_marker_checked)
  (task_list_marker_unchecked)
] @keyword

(link_destination) @comment
(link_label) @link

[
  (atx_heading)
  (setext_heading)
] @heading
