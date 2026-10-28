(plugin git
  :version "0.4.4"
  :opt-in true
  :summary "Git status and staging in the style of Magit"
  :commands {git-status: "Show the Git status of the current repository"
             git-commit-finish: "Commit with this message"
             git-commit-cancel: "Cancel this commit"
             git-commit-previous: "Show the previous commit message"
             git-commit-next: "Show the next commit message"}
  :keys {"C-x g" git-status}
  :modes [{title: "Git commit" file-names: ["COMMIT_EDITMSG"] comment: "#"}]
  :mode-keys {"Git commit" {"C-c C-c" git-commit-finish "C-c C-k" git-commit-cancel
                            "M-p" git-commit-previous "M-n" git-commit-next}}
  :entry "git.ox")
