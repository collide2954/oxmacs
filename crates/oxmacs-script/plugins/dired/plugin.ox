(plugin dired
  :version "0.3.4"
  :summary "List directories and manage their files"
  :commands [dired dired-jump]
  :keys {"C-x d" dired "C-x C-j" dired-jump}
  :start-screen {dired: "Browse a directory"}
  :opens-directories true
  :entry "dired.ox")
