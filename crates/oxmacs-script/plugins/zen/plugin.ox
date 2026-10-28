(plugin zen
  :version "0.1.2"
  :opt-in true
  :summary "Distraction-free writing in one centered column"
  :commands [zen]
  :settings {width: {type: :integer default: 80 doc: "How many characters wide the text column is"}
             hide-chrome: {type: :toggle default: true doc: "Show one window without mode lines, line numbers, tab bar or side window; the windows come back afterwards"}
             dim: {type: :toggle default: true doc: "Fade every paragraph but the one you are in"}
             typewriter: {type: :toggle default: true doc: "Keep the line you are on in the middle of the window"}
             fullscreen: {type: :toggle default: true doc: "Fill the whole screen"}
             wrap-words: {type: :toggle default: true doc: "Wrap long lines between words"}}
  :entry "zen.ox")
