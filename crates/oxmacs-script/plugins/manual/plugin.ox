(plugin manual
  :version "0.5.100"
  :requires {markdown: "^0.2"}
  :summary "The oxmacs tutorial and the Ox manual"
  :commands [ox-manual ox-manual-lookup oxmacs-tutorial]
  :keys {"C-h i" ox-manual "C-h t" oxmacs-tutorial}
  :start-screen {oxmacs-tutorial: "Learn oxmacs" ox-manual: "Read the Ox manual"}
  :entry "manual.ox")
