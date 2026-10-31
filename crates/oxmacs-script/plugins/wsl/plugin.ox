(plugin wsl
  :version "0.1.1"
  :opt-in true
  :startup-when true
  :requires {remote: "^0.4"}
  :summary "WSL distributions as places to work: their files open as your own, their tools run inside them, and *WSL* starts, stops and opens them"
  :commands [wsl wsl-open-home wsl-start wsl-terminate wsl-set-default wsl-shutdown wsl-shell wsl-run wsl-find-file wsl-copy-linux-path]
  :entry "wsl.ox")
