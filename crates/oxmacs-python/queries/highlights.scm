; Adapted from tree-sitter-python's highlights.scm (MIT).
; When patterns capture the same text, the later one wins.

((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_0-9]*$"))

(call
  function: (attribute
    attribute: (identifier) @function))

(call
  function: (identifier) @function)

(function_definition
  name: (identifier) @function)

(class_definition
  name: (identifier) @type)

(type
  (identifier) @type)

[
  (none)
  (true)
  (false)
  (integer)
  (float)
] @constant

[
  "and"
  "as"
  "assert"
  "async"
  "await"
  "break"
  "case"
  "class"
  "continue"
  "def"
  "del"
  "elif"
  "else"
  "except"
  "exec"
  "finally"
  "for"
  "from"
  "global"
  "if"
  "import"
  "in"
  "is"
  "is not"
  "lambda"
  "match"
  "nonlocal"
  "not"
  "not in"
  "or"
  "pass"
  "print"
  "raise"
  "return"
  "try"
  "while"
  "with"
  "yield"
] @keyword

(decorator) @attribute

(comment) @comment

(string) @string

(module
  .
  (expression_statement
    (string) @comment.documentation))

(class_definition
  body: (block
    .
    (expression_statement
      (string) @comment.documentation)))

(function_definition
  body: (block
    .
    (expression_statement
      (string) @comment.documentation)))
