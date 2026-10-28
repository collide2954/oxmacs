(plugin pdf
  :version "0.2.0"
  :opt-in true
  :summary "Read PDF documents drawn by PDFium, with search, an outline, links and copying"
  :opens-files ["pdf"]
  :settings {zoom: {type: [:fit-width :fit-page :actual] default: :fit-width doc: "How large pages are when a document opens"}
             continuous: {type: :bool default: true doc: "Scroll through every page, or show one page at a time"}
             theme-colors: {type: :bool default: false doc: "Draw pages in the theme's colors instead of black on white"}
             library: {type: :string doc: "A PDFium library to use instead of the one M-x list-tools installs"}}
  :entry "pdf.ox")
