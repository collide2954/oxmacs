; Adapted from tree-sitter-rust's highlights.scm (MIT).
; When patterns capture the same text, the later one wins.

(line_comment) @comment
(block_comment) @comment
(line_comment (doc_comment)) @comment.documentation
(block_comment (doc_comment)) @comment.documentation

(string_literal) @string
(raw_string_literal) @string
(char_literal) @string

(boolean_literal) @constant
(integer_literal) @constant
(float_literal) @constant

(attribute_item) @attribute
(inner_attribute_item) @attribute

(lifetime) @lifetime

(call_expression
  function: (identifier) @function)
(call_expression
  function: (field_expression
    field: (field_identifier) @function))
(call_expression
  function: (scoped_identifier
    name: (identifier) @function))
(generic_function
  function: (identifier) @function)
(generic_function
  function: (scoped_identifier
    name: (identifier) @function))
(generic_function
  function: (field_expression
    field: (field_identifier) @function))

((identifier) @type
  (#match? @type "^[A-Z][a-z]"))
((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))
((scoped_identifier
  path: (identifier) @type)
  (#match? @type "^[A-Z]"))
((scoped_type_identifier
  path: (identifier) @type)
  (#match? @type "^[A-Z]"))

(function_item
  name: (identifier) @function)
(function_signature_item
  name: (identifier) @function)
(const_item
  name: (identifier) @constant)
(static_item
  name: (identifier) @constant)
(macro_definition
  name: (identifier) @function.macro)
(macro_invocation
  macro: (identifier) @function.macro
  "!" @function.macro)
(macro_invocation
  macro: (scoped_identifier
    name: (identifier) @function.macro)
  "!" @function.macro)

(type_identifier) @type
(primitive_type) @type

[
  "as"
  "async"
  "await"
  "break"
  "const"
  "continue"
  "default"
  "dyn"
  "else"
  "enum"
  "extern"
  "fn"
  "for"
  "gen"
  "if"
  "impl"
  "in"
  "let"
  "loop"
  "macro_rules!"
  "match"
  "mod"
  "move"
  "pub"
  "raw"
  "ref"
  "return"
  "static"
  "struct"
  "trait"
  "type"
  "union"
  "unsafe"
  "use"
  "where"
  "while"
  "yield"
] @keyword
(crate) @keyword
(super) @keyword
(self) @keyword
(mutable_specifier) @keyword
