(plugin epub
  :version "0.1.4"
  :opt-in true
  :summary "Read EPUB books in pages, with contents, search, highlights, notes and a library"
  :commands {epub-library: "Show the books in a folder as covers"
             epub-recent: "Open a book read lately"}
  :opens-files ["epub"]
  :settings {typeface: {type: [:serif :sans :mono] default: :serif doc: "The typeface books are set in"}
             size: {type: :int default: 19 doc: "The size of the text in pixels; + and - change it while reading"}
             spread: {type: [:auto :single :double] default: :auto doc: "Two pages side by side when the window is wide enough, always one, or always two"}
             library: {type: :string doc: "The folder M-x epub-library shows; it asks when unset"}
             highlight: {type: [:marked :added :flagged :link] default: :marked doc: "The face highlights are drawn in"}}
  :entry "epub.ox")
