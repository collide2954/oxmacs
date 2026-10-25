(plugin zen
  :version "0.1.0"
  :opt-in true
  :summary "Distraction-free writing in one centered column"
  :commands {zen: "Write without distractions in one centered column, or come back"}
  :settings {width: {type: :int default: 80 doc: "How many characters wide the text column is"}
             hide-chrome: {type: :bool default: true doc: "Show one window without mode lines, line numbers, tab bar or side window; the windows come back afterwards"}
             dim: {type: :bool default: true doc: "Fade every paragraph but the one you are in"}
             typewriter: {type: :bool default: true doc: "Keep the line you are on in the middle of the window"}
             fullscreen: {type: :bool default: true doc: "Fill the whole screen"}
             wrap-words: {type: :bool default: true doc: "Wrap long lines between words"}}
  :entry "zen.ox")
