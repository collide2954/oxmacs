(plugin zap
  :version "0.2.2"
  :opt-in true
  :summary "Flash the code you evaluate or send to a console, in the error color when it fails"
  :settings {flash: {type: :toggle default: true doc: "Flash code as it runs; false leaves zap off"}
             duration: {type: :integer default: 500 doc: "Milliseconds a flash takes to fade out"}
             face: {type: [:marked :region :added :success :link :warning] default: :marked doc: "The face code that ran flashes in"}
             error-face: {type: [:flagged :removed :error :warning] default: :flagged doc: "The face code that failed flashes in"}}
  :startup-when :flash
  :entry "zap.ox")
