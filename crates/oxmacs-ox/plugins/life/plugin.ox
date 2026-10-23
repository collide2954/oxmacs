(plugin life
  :version "0.2.0"
  :summary "Conway's Game of Life on a canvas"
  :commands {life: "Watch Conway's Game of Life"}
  :settings {pattern: {type: [:acorn :r-pentomino :glider :random] default: :acorn doc: "The pattern life starts from"}
             speed: {type: :int default: 100 doc: "Milliseconds between generations"}}
  :opt-in true
  :entry "life.ox")
