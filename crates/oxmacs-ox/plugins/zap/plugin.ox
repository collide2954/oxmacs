(plugin zap
  :version "0.1.0"
  :opt-in true
  :summary "Flash the code you evaluate or send to a console, in the error color when it fails"
  :settings {flash: {type: :bool default: true doc: "Flash code as it runs; false leaves zap off"}
             duration: {type: :int default: 500 doc: "Milliseconds a flash takes to fade out"}}
  :startup-when :flash
  :entry "zap.ox")
