; Adapted from tree-sitter-toml's highlights.scm (MIT).
; When patterns capture the same text, the later one wins.

(comment) @comment

(string) @string

[
  (integer)
  (float)
  (boolean)
  (offset_date_time)
  (local_date_time)
  (local_date)
  (local_time)
] @constant

(pair
  [
    (bare_key)
    (quoted_key)
    (dotted_key)
  ] @function)

(table
  "[" @type
  [
    (bare_key)
    (quoted_key)
    (dotted_key)
  ] @type
  "]" @type)

(table_array_element
  "[[" @type
  [
    (bare_key)
    (quoted_key)
    (dotted_key)
  ] @type
  "]]" @type)
