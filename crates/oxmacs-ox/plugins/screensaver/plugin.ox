(plugin screensaver
  :version "0.1.1"
  :summary "A starfield or matrix rain after a while without input"
  :commands {screensaver-preview: "Show a screensaver until the next key or click"}
  :settings {idle: {type: :int doc: "Seconds without input before a screensaver starts; unset leaves screensavers off"}
             kind: {type: [:random :starfield :matrix] default: :random doc: "Which screensaver starts when oxmacs is idle"}}
  :startup-when :idle
  :entry "screensaver.ox")
