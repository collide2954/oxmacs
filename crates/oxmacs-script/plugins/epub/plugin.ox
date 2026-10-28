(plugin epub
  :version "0.2.5"
  :opt-in true
  :summary "Read EPUB books in pages, with contents, search, highlights, notes and a library"
  :commands [epub-library epub-recent]
  :opens-files ["epub"]
  :settings {typeface: {type: [:serif :sans :mono] default: :serif doc: "The typeface books are set in"}
             size: {type: :integer default: 19 doc: "The size of the text in pixels; + and - change it while reading"}
             spread: {type: [:auto :single :double] default: :auto doc: "Two pages side by side when the window is wide enough, always one, or always two"}
             library: {type: :text doc: "The folder M-x epub-library shows; it asks when unset"}
             highlight: {type: [:marked :added :flagged :link] default: :marked doc: "The face highlights are drawn in"}}
  :entry "epub.ox")
