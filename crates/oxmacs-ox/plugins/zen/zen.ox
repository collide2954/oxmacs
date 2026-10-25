(module
  (import (zen look :as look)))

(defonce on (atom false))
(defonce wrapped (atom nil))

(def (wanted)
  (look/chosen {width: (setting :width)
                hide-chrome: (setting :hide-chrome)
                dim: (setting :dim)
                typewriter: (setting :typewriter)
                fullscreen: (setting :fullscreen)}))

(def (enter)
  (reset! on true)
  (select-main-window)
  (when (setting :hide-chrome)
    (store-windows :zen)
    (arrange-windows (buffer-name)))
  (when (setting :wrap-words)
    (reset! wrapped [(current-buffer) (word-wrap?)])
    (set-word-wrap (current-buffer) true))
  (set-presentation (wanted))
  (message "Zen mode; M-x zen comes back"))

(def (leave)
  (reset! on false)
  (set-presentation nil)
  (when-let [kept @wrapped]
    (let [[buffer before] kept]
      (when (buffer-live? buffer) (set-word-wrap buffer before))))
  (reset! wrapped nil)
  (recall-windows :zen)
  (message "Zen mode off"))

(defcommand zen "Write without distractions in one centered column, or come back"
  (if @on (leave) (enter)))

(on-settings-change :zen
                    (fn []
                      (when @on (set-presentation (wanted)))))
