((inline) @injection.content
  (#set! injection.language "markdown_inline")
  (#set! injection.combined))

(fenced_code_block
  (info_string
    (language) @injection.language)
  (code_fence_content) @injection.content)
