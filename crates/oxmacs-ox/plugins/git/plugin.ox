(plugin git
  :version "0.2.0"
  :summary "Git status and staging in the style of Magit"
  :commands {git-status: "Show the Git status of the current repository"}
  :keys {"C-x g" git-status}
  :modes [{name: "Git commit" file-names: ["COMMIT_EDITMSG"] comment: "#"}]
  :settings {discard-to-trash: {type: :bool default: true doc: "Move discarded untracked files to the trash instead of deleting them"}}
  :entry "git.ox")
