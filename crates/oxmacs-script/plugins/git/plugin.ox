(plugin git
  :version "0.4.5"
  :opt-in true
  :summary "Git status and staging in the style of Magit"
  :commands [git-status git-commit-finish git-commit-cancel git-commit-previous git-commit-next]
  :keys {"C-x g" git-status}
  :modes [{title: "Git commit" file-names: ["COMMIT_EDITMSG"] comment: "#"}]
  :mode-keys {"Git commit" {"C-c C-c" git-commit-finish "C-c C-k" git-commit-cancel
                            "M-p" git-commit-previous "M-n" git-commit-next}}
  :entry "git.ox")
