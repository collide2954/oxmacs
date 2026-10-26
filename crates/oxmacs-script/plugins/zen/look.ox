(module
  (export narrowest chosen))

(def narrowest 20)

(def (chosen settings)
  "What zen mode asks the windows to look like with settings: a text column of :width characters, at least narrowest, and each of :hide-chrome, :dim, :typewriter and :fullscreen only when turned on."
  {width: (max narrowest (or settings.width 80))
   hide-chrome: (= settings.hide-chrome true)
   dim: (= settings.dim true)
   typewriter: (= settings.typewriter true)
   fullscreen: (= settings.fullscreen true)})

(test "zen asks for what its settings turn on"
  (expect (= (chosen {width: 72 hide-chrome: true dim: false typewriter: true fullscreen: nil})
             {width: 72 hide-chrome: true dim: false typewriter: true fullscreen: false})))

(test "the column never gets too narrow"
  (expect (= (get (chosen {width: 3}) :width) narrowest))
  (expect (= (get (chosen {}) :width) 80)))
