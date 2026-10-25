(plugin manual
  :version "0.5.22"
  :requires {markdown: "^0.2"}
  :summary "The oxmacs tutorial and the Ox manual"
  :commands {ox-manual: "Read the Ox manual"
             ox-manual-lookup: "Look up an Ox name in the manual"
             oxmacs-tutorial: "Learn oxmacs hands-on"}
  :keys {"C-h i" ox-manual "C-h t" oxmacs-tutorial}
  :start-screen {oxmacs-tutorial: "Learn oxmacs" ox-manual: "Read the Ox manual"}
  :entry "manual.ox")
