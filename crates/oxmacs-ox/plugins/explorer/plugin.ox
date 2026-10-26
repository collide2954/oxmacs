(plugin explorer
  :version "0.1.1"
  :summary "A file tree beside the windows that follows the file you're in"
  :commands {explorer: "Show the file tree beside the windows, move to it, or hide it"}
  :keys {"C-x C-d" explorer}
  :settings {width: {type: :int default: 32 doc: "How many columns the tree takes"}
             edge: {type: [:left :right] default: :left doc: "Which side of the frame the tree sits on"}
             follow: {type: :bool default: true doc: "Move to the project of each file you switch to, not only to files inside the one shown"}
             hidden: {type: :bool default: false doc: "Show hidden files and the files git ignores"}
             delete-to-trash: {type: :bool default: true doc: "Delete to the trash instead of permanently"}}
  :entry "explorer.ox")
