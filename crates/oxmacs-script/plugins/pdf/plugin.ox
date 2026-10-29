(plugin pdf
  :version "0.2.4"
  :opt-in true
  :summary "Read PDF documents drawn by PDFium, with search, an outline, links and copying"
  :opens-files ["pdf"]
  :settings {zoom: {type: [:fit-width :fit-page :actual] default: :fit-width doc: "How large pages are when a document opens"}
             continuous: {type: :toggle default: true doc: "Scroll through every page, or show one page at a time"}
             theme-colors: {type: :toggle default: false doc: "Draw pages in the theme's colors instead of black on white"}
             library: {type: :text doc: "A PDFium library to use instead of the one M-x list-tools installs"}}
  :tools [{name: "pdfium" repository: "bblanchon/pdfium-binaries" summary: "Draws pages for the PDF viewer"
           group: "PDF" form: :library tag-prefix: "chromium/" checksum: :digest setting: :library
           archives: {windows: {x86_64: "pdfium-win-x64.tgz" aarch64: "pdfium-win-arm64.tgz"}
                      macos: {x86_64: "pdfium-mac-x64.tgz" aarch64: "pdfium-mac-arm64.tgz"}
                      linux: {x86_64: "pdfium-linux-x64.tgz" aarch64: "pdfium-linux-arm64.tgz"}}}]
  :entry "pdf.ox")
