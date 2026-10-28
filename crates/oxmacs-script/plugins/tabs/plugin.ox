(plugin tabs
  :version "0.1.4"
  :opt-in true
  :summary "Workspace tabs across the top, each keeping its own windows"
  :commands [tab-new tab-close tab-close-other tab-next tab-previous tab-rename tab-switch
             tab-move tab-duplicate tab-undo find-file-other-tab switch-to-buffer-other-tab]
  :keys {"C-x t 2" tab-new "C-x t 0" tab-close "C-x t 1" tab-close-other
         "C-x t o" tab-next "C-TAB" tab-next "C-x t O" tab-previous "C-S-TAB" tab-previous
         "C-x t r" tab-rename "C-x t RET" tab-switch "C-x t m" tab-move
         "C-x t n" tab-duplicate "C-x t u" tab-undo
         "C-x t f" find-file-other-tab "C-x t b" switch-to-buffer-other-tab}
  :settings {show: {type: [:several :always] default: :several doc: "Show the tab bar once there are two tabs, or always"}
             new-tab: {type: [:current :scratch] default: :current doc: "What a new tab shows: the current buffer or *scratch*"}}
  :startup-when true
  :entry "tabs.ox")
