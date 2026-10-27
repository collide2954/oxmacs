(module
  (import (ox file)
          (ox string)))

(def starter
  #r##"(module (import (ox rodeo)))

(rodeo/screen 320 180)
(rodeo/actions {left: [:left :a] right: [:right :d] jump: [:space :up :w]})

(def ground 150)
(def speed 90)
(def gravity 600)
(def jump -260)

(defonce player (atom {x: 40 y: ground vy: 0}))

(def (tick)
  (let [p @player
        dt (rodeo/dt)
        across (cond ((rodeo/held? :left) (- speed)) ((rodeo/held? :right) speed) (else 0))
        standing (>= p.y ground)
        vy (if (and standing (rodeo/pressed? :jump)) jump (+ p.vy (* gravity dt)))
        y (min ground (+ p.y (* vy dt)))]
    (reset! player {x: (+ p.x (* across dt)) y: y vy: (if (>= y ground) 0 vy)})))

(def (draw)
  (let [p @player]
    (rodeo/clear "#1d2b53")
    (rodeo/rect [0 (+ ground 16) 320 30] "#008751")
    (rodeo/rect [p.x p.y 12 16] "#ff004d")
    (rodeo/text "Arrows move, space jumps" [6 6] :color "#fff1e8")))
"##)

(defonce slowed (atom false))

(def (game-file)
  (let [path (buffer-file)]
    (and path (string/ends-with? path ".ox") path)))

(defcommand rodeo-play "Play the game in this Ox file in a window beside it"
  (if-let [path (game-file)]
    (do (run-command 'save-buffer)
      (reset! slowed false)
      (play-game path))
    (message "Visit a game's .ox file first; M-x rodeo-new writes one to start from")))

(defcommand rodeo-pause "Pause the running game, or carry on"
  (if (game-playing?)
    (pause-game)
    (resume-game)))

(defcommand rodeo-step "Run one tick of the paused game"
  (step-game))

(defcommand rodeo-slow "Run the game at a quarter of its speed, or back at full speed"
  (game-speed (if (swap! slowed not) 0.25 1)))

(defcommand rodeo-restart "Load the game's file again and call its start"
  (restart-game))

(defcommand rodeo-stop "Stop the game and close its window"
  (stop-game))

(defcommand rodeo-new "Write a small game to start from, and visit it"
  (let [path (await (prompt-path "New game file: " :initial "game.ox"))]
    (if (await (file/exists? path))
      (message "{path} already exists; pick another name")
      (do (await (file/write path starter))
        (open-file path)))))

(test "the starter game reads as Ox"
  (expect (string/includes? starter "(def (tick)")))
