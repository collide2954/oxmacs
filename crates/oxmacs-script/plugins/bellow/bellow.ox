(module
  (import (ox file)
          (ox string)
          (ox ui :as ui)
          (ox synth :as s)))

(def workbench "*bellow*")

(def starter
  #r##"(module (import (ox synth :as s)))

(def pad
  (-> (s/mix (s/saw :detune -7) (s/saw :detune 7))
      (s/lowpass (s/control :cutoff 1200 :low 200 :high 6000) :resonance 0.2)
      (s/envelope :attack 0.8 :release 2)))

(def bell
  (-> (s/mix (s/sine) (s/gain (s/sine :octave 2) 0.25))
      (s/envelope :attack 0.005 :decay 1.2 :sustain 0 :release 1.2)))

(def song
  (s/song :tempo 72
    (s/track :pad pad (s/chords "Dm9 _ | Bbmaj7 _ | F _ | C _")
             :effects (s/reverb (s/input) :size 0.9 :mix 0.4))
    (s/track :bell bell (s/notes "a5 . e5 . | f5 . _ . | c6 . a5 . | g5 _ _ .")
             :effects (s/delay (s/input) 0.42 :cross true))))

(s/loop song)
"##)

(defonce ticks (atom 0))
(defonce ticking (atom false))

(def (shown value high)
  (if (> high 10)
    (str (round value))
    (str (/ (round (* value 100)) 100))))

(def (control-row control)
  (ui/row (ui/text (string/pad-end (name control.name) 12) :face :function)
          (ui/slider control.value control.low control.high
                     :on-change (fn [value] (s/set control.name value))
                     :width 280)
          (ui/text (shown control.value control.high))
          (when control.automated (ui/text "~ automated" :face :comment))
          :spacing 12))

(def (scope-lines points height)
  (let [last (- (count points) 1)
        middle (/ height 2)
        y (fn [value] (- middle (* value (- middle 2))))]
    (for [at (range last)]
      [:line (/ at last) (y (nth points at)) (/ (+ at 1) last) (y (nth points (+ at 1))) :string 2])))

(def (heading)
  (if-let [at (s/position)]
    "bar {(+ 1 (floor at.bar))}   {(round at.tempo)} bpm"
    "stopped"))

(def (render)
  @ticks
  (let [controls (s/controls)]
    (ui/column (ui/text (heading) :face :keyword :size 18)
               (ui/canvas (scope-lines (s/scope 160) 72) :height 72 :relative true)
               (ui/canvas [[:rect 0 2 (min 1 (* 1.5 (s/level))) 6 :string]] :height 10 :relative true)
               (if (empty? controls)
                 (ui/text "No controls yet: (s/control :cutoff 1200) in a sound adds a slider here" :face :comment)
                 (for [control controls] (control-row control)))
               (ui/text "Sliders, (s/set :name value) and (s/automate :name [[bar value] ...]) move the same controls"
                        :face :comment)
               :spacing 10)))

(def (keep-fresh)
  (unless @ticking
    (reset! ticking true)
    (spawn
      (do (while (buffer-exists? workbench)
            (swap! ticks inc)
            (sleep (if (s/position) 100 1000)))
        (reset! ticking false)))))

(def (show-workbench)
  (open-view workbench render :beside true)
  (keep-fresh))

(def (song-file)
  (let [path (buffer-file)]
    (and path (string/ends-with? path ".ox") path)))

(defcommand bellow-play "Play the song in this Ox file and show the workbench beside it"
  (if (song-file)
    (do (run-command 'ox-eval-buffer)
      (show-workbench))
    (message "Visit a song's .ox file first; M-x bellow-new writes one to start from")))

(defcommand bellow-stop "Stop the song and every sound playing"
  (s/stop)
  (message "Stopped"))

(defcommand bellow-workbench "Show the song's position, its sound and a slider for each control"
  (show-workbench))

(defcommand bellow-new "Write a small song to start from, and visit it"
  (let [path (await (prompt-path "New song file: " :initial "song.ox"))]
    (if (await (file/exists? path))
      (message "{path} already exists; pick another name")
      (do (await (file/write path starter))
        (open-file path)))))

(test "the starter song reads as Ox and loops itself"
  (expect (string/includes? starter "(s/loop song)")))

(test "values show whole above 10 and to two places below"
  (expect (= (shown 1834.4 6000) "1834"))
  (expect (= (shown 0.256 1) "0.26")))
