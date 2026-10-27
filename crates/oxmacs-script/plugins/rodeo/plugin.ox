(plugin rodeo
  :version "0.1.1"
  :summary "Play 2D games written in Ox beside their code, and change them while they run"
  :commands {rodeo-play: "Play the game in this Ox file in a window beside it"
             rodeo-pause: "Pause the running game, or carry on"
             rodeo-step: "Run one tick of the paused game"
             rodeo-slow: "Run the game at a quarter of its speed, or back at full speed"
             rodeo-restart: "Load the game's file again and call its start"
             rodeo-stop: "Stop the game and close its window"
             rodeo-new: "Write a small game to start from, and visit it"}
  :opt-in true
  :entry "rodeo.ox")
