(module
  (import (ox time)
          (screensaver savers :as savers)))

(def (chosen)
  (let [kind (setting :kind)]
    (if (= kind :random)
      (get (nth savers/savers (mod (int (/ (time/now) 1000)) (count savers/savers))) :kind)
      kind)))

(defcommand screensaver-preview "Show a screensaver until the next key or click"
  (let [answer (await (prompt "Screensaver: " :choices savers/notes))
        kind (find (fn [known] (= (name known) answer)) (keys savers/notes))]
    (if kind
      (savers/start kind)
      (message $"There is no screensaver named \"{answer}\""))))

(def (wait-for-idle)
  (when-let [seconds (setting :idle)]
    (on-idle :screensaver seconds (fn [] (when (setting :idle) (savers/start (chosen)))))))

(wait-for-idle)
(on-settings-change :screensaver wait-for-idle)
