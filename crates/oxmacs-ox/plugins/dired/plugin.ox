(plugin dired
  :version "0.2.3"
  :summary "List directories and manage their files"
  :commands {dired: "List a directory to manage its files"
             dired-jump: "List the directory of the current file"}
  :keys {"C-x d" dired "C-x C-j" dired-jump}
  :start-screen {dired: "Browse a directory"}
  :opens-directories true
  :settings {delete-to-trash: {type: :bool default: true doc: "Delete to the trash instead of permanently"}}
  :entry "dired.ox")
