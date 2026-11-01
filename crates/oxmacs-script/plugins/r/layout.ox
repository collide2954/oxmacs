(module
  (import (r session)
          (r plots)
          (r data)))

(defonce stored (atom false))

(def (script-for running)
  (let [scripts (filter session/script? (buffer-list))]
    (if (session/script? (buffer-name))
      (buffer-name)
      (or (find (fn [name] (= (session/linked name) running)) scripts) (first scripts)))))

(defcommand r-layout "Arrange the windows like RStudio: script and console, environment and plots"
  (let [here-is-r (session/script? (buffer-name))]
    (when-let [running (session/ensure)]
      (when here-is-r (session/link! running))
      (let [script (script-for running)
            environment (data/environment-buffer running)
            corner (or (plots/unpinned-of running)
                       (when (buffer-live? "*R help*") "*R help*")
                       (plots/plots-buffer running))
            left (if script [:rows [0.6 script] [0.4 running]] running)]
        (unless @stored
          (store-windows :r-layout)
          (reset! stored true))
        (arrange-windows [:columns [0.5 left] [0.5 [:rows [0.4 environment] [0.6 corner]]]]
                         :select (or script running))
        (data/ask-environment running)))))

(defcommand r-layout-restore "Put the windows back as they were before r-layout"
  (if @stored
    (do (reset! stored false)
      (recall-windows :r-layout))
    (message "No layout to restore")))
