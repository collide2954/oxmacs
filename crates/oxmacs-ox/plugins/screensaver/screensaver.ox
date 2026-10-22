(module
  (import (ox time)
          (screensaver starfield :as stars)
          (screensaver matrix :as rain)))

(def notes
  {starfield: "Stars rushing toward you"
   matrix: "Glyphs raining down the screen"})

(def (start kind)
  (cond ((= kind :matrix) (take-over rain/draw :fps 15))
        (else (take-over stars/draw))))

(def (chosen)
  (let [kind (setting :kind)]
    (cond ((= kind :random) (if (even? (time/now)) :starfield :matrix))
          (else kind))))

(defcommand screensaver-preview "Show a screensaver until the next key or click"
  (let [answer (await (prompt "Screensaver: " :choices notes))
        kind (find (fn [known] (= (name known) answer)) (keys notes))]
    (if kind
      (start kind)
      (message "There is no screensaver named \"{answer}\""))))

(when-let [seconds (setting :idle)]
  (on-idle :screensaver seconds (fn [] (start (chosen)))))