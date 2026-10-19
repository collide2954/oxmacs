; Adapted from tree-sitter-r's highlights.scm (MIT).
; When patterns capture the same text, the later one wins.

[
  (integer)
  (float)
  (complex)
  (true)
  (false)
  (null)
  (inf)
  (nan)
  (na)
  (dots)
  (dot_dot_i)
] @constant

(namespace_operator
  lhs: (identifier) @type)

(call
  function: (identifier) @function)

(call
  function: (namespace_operator
    rhs: (identifier) @function))

(binary_operator
  lhs: (identifier) @function
  operator: ["<-" "="]
  rhs: (function_definition))

((call
  function: (identifier) @keyword)
  (#eq? @keyword "return"))

(function_definition
  name: "function" @keyword)

[
  "if"
  "else"
  "while"
  "repeat"
  "for"
  "in"
  (next)
  (break)
] @keyword

(string) @string

(comment) @comment

((comment) @comment.documentation
  (#match? @comment.documentation "^#'"))