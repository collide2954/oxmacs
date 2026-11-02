(plugin git
  :version "0.6.0"
  :opt-in true
  :summary "Git status, staging, history and branches in the style of Magit"
  :commands [git-status git-log git-branches git-process
             git-commit-finish git-commit-cancel git-commit-previous git-commit-next]
  :keys {"C-x g" git-status}
  :settings {mode-line: {type: :toggle default: false
                         doc: "The repository's branch in the mode line, as main ↑2 ↓0 ●; clicking it opens the status"}}
  :startup-when :mode-line
  :modes [{title: "Git commit" file-names: ["COMMIT_EDITMSG"] comment: "#"}]
  :mode-keys {"Git commit" {"C-c C-c" git-commit-finish "C-c C-k" git-commit-cancel
                            "M-p" git-commit-previous "M-n" git-commit-next}}
  :entry "git.ox")
