(module
  (import (ox ui)
          (life rules :as rules)))

(def buffer "*Life*")
(def cell-height 9)

(defonce world (atom {cells: [] generation: 0 running: false run: 0}))

(def (shown? [x y])
  (and (>= x 0) (< x rules/width) (>= y 0) (< y rules/height)))

(def (draw)
  (let [now @world]
    (ui/column
     (ui/row (ui/text (str "Generation " now.generation "   " (count now.cells) " alive") :face :heading)
             (ui/space)
             (ui/text (str (if now.running "" "Paused   ") "SPC pauses  n steps  r restarts  q quits") :face :comment))
     (ui/canvas (vec (map (fn [[x y]]
                            [:rect (/ x rules/width) (* y cell-height) (/ 0.9 rules/width) (dec cell-height) :success])
                          (filter shown? (rules/points now.cells))))
                :height (* rules/height cell-height) :relative true)
     :spacing 6)))

(def (advance!)
  (swap! world (fn [now] (merge now {cells: (rules/step now.cells) generation: (inc now.generation)}))))

(def (restart!)
  (swap! world merge {cells: (rules/seeded (setting :pattern)) generation: 0 running: true}))

(def (run!)
  (let [run (get (swap! world update :run inc) :run)]
    (every (setting :speed)
           (fn []
             (let [now @world]
               (cond ((not= now.run run) :stop)
                 ((not (buffer-shown? buffer)) (do (swap! world assoc :running false) :stop))
                 (now.running (do (advance!) nil))
                 (else nil)))))))

(defcommand life "Watch Conway's Game of Life"
  (restart!)
  (open-view buffer draw :keys {"SPC" 'life-pause "n" 'life-step "r" 'life-restart "q" 'life-quit})
  (run!))

(defcommand life-pause "Pause the Game of Life, or carry on"
  (if (get @world :running)
    (swap! world assoc :running false)
    (do (swap! world assoc :running true)
      (run!))))

(defcommand life-step "Show the next generation"
  (swap! world assoc :running false)
  (advance!))

(defcommand life-restart "Start the Game of Life again from its pattern"
  (restart!)
  (run!))

(defcommand life-quit "Stop the Game of Life and close it"
  (swap! world update :run inc)
  (close-buffer buffer))
