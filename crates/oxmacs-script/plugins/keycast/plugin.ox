(plugin keycast
  :version "0.2.4"
  :opt-in true
  :summary "Shows the keys you press and the commands they run, for demos"
  :settings {placement: {type: [:bottom-right :bottom-center :mode-line] default: :bottom-right doc: "Where keys show: a pill at the bottom right, a larger one at the bottom center for videos watched on phones, or the mode line"}}
  :startup-when true
  :entry "keycast.ox")
