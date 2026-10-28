(plugin git
  :version "0.4.2"
  :opt-in true
  :summary "Git status and staging in the style of Magit"
  :commands {git-status: "Show the Git status of the current repository"}
  :keys {"C-x g" git-status}
  :modes [{name: "Git commit" file-names: ["COMMIT_EDITMSG"] comment: "#"}]
  :entry "git.ox")
