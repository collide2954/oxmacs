(plugin pixi
  :version "0.1.0"
  :opt-in true
  :startup-when true
  :summary "pixi projects as environments: programs started in one run in its pixi environment, with its tasks, packages and environments on *pixi*"
  :commands [pixi pixi-select pixi-run pixi-environment pixi-add pixi-remove pixi-install pixi-update pixi-install-pixi]
  :entry "pixi.ox")
