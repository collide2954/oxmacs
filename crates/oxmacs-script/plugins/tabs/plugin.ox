(plugin tabs
  :version "0.1.1"
  :opt-in true
  :summary "Workspace tabs across the top, each keeping its own windows"
  :commands {tab-new: "Open a new tab"
             tab-close: "Close the current tab"
             tab-close-other: "Close every tab but the current one"
             tab-next: "Go to the next tab"
             tab-previous: "Go to the previous tab"
             tab-rename: "Name the current tab"
             tab-switch: "Go to a tab by its name"
             tab-move: "Move the current tab one place to the right"
             tab-duplicate: "Open a new tab with the current windows"
             tab-undo: "Bring back the last closed tab"
             find-file-other-tab: "Visit a file in a new tab"
             switch-to-buffer-other-tab: "Show a buffer in a new tab"}
  :keys {"C-x t 2" tab-new "C-x t 0" tab-close "C-x t 1" tab-close-other
         "C-x t o" tab-next "C-TAB" tab-next "C-x t O" tab-previous "C-S-TAB" tab-previous
         "C-x t r" tab-rename "C-x t RET" tab-switch "C-x t m" tab-move
         "C-x t n" tab-duplicate "C-x t u" tab-undo
         "C-x t f" find-file-other-tab "C-x t b" switch-to-buffer-other-tab}
  :settings {show: {type: [:several :always] default: :several doc: "Show the tab bar once there are two tabs, or always"}
             new-tab: {type: [:current :scratch] default: :current doc: "What a new tab shows: the current buffer or *scratch*"}}
  :startup-when :show
  :entry "tabs.ox")
