(plugin screensaver
  :version "0.2.4"
  :opt-in true
  :summary "A starfield or matrix rain after a while without input"
  :commands [screensaver-preview]
  :settings {idle: {type: :integer doc: "Seconds without input before a screensaver starts; unset leaves screensavers off"}
             kind: {type: [:random :starfield :matrix] default: :random doc: "Which screensaver starts when oxmacs is idle"}}
  :startup-when :idle
  :entry "screensaver.ox")
