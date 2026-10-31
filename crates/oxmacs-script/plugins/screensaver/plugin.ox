(plugin screensaver
  :version "0.3.1"
  :opt-in true
  :summary "Twenty-nine screensavers after a while without input, from a starfield and matrix rain to fireworks, an aurora, a lava lamp and a Mandelbrot dive"
  :commands [screensaver-preview]
  :settings {idle: {type: :integer doc: "Seconds without input before a screensaver starts; unset leaves screensavers off"}
             kind: {type: [:random :starfield :matrix :pond :fireworks :harmonograph :aurora :clock :flow :orbits :snowfall :tunnel :lava :constellations :plasma :bubbles :kaleidoscope :wireframes :city :pendulums :tree :ocean :glass :galaxy :ants :campfire :sorting :storm :fireflies :mandelbrot] default: :random doc: "Which screensaver starts when oxmacs is idle"}}
  :startup-when :idle
  :entry "screensaver.ox")
