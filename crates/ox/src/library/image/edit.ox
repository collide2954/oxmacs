(def (edit image change)
  "Calls change with a canvas holding a private copy of image, so the drawing functions ending in ! can change it in place, then returns the edited image."
  (let [canvas (begin-edit image)]
    (change canvas)
    (end-edit canvas)))
