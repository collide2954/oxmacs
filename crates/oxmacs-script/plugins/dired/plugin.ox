(plugin dired
  :version "0.3.0"
  :summary "List directories and manage their files"
  :commands {dired: "List a directory to manage its files"
             dired-jump: "List the directory of the current file"}
  :keys {"C-x d" dired "C-x C-j" dired-jump}
  :start-screen {dired: "Browse a directory"}
  :opens-directories true
  :entry "dired.ox")
