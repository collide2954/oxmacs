(module
  (import (r session :as session)
          (r plots :as plots)
          (r data :as data)))

(defonce saved (atom nil))

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
                       (when (buffer-exists? "*R help*") "*R help*")
                       (plots/plots-buffer running))
            left (if script [:rows [0.6 script] [0.4 running]] running)]
        (unless @saved (reset! saved (window-layout)))
        (arrange-windows [:columns [0.5 left] [0.5 [:rows [0.4 environment] [0.6 corner]]]]
                         :select (or script running))
        (data/ask-environment running)))))

(defcommand r-layout-restore "Put the windows back as they were before r-layout"
  (if-let [layout @saved]
    (do (reset! saved nil)
      (try (arrange-windows layout)
        (catch error e (message "Cannot restore the layout: {e.message}"))))
    (message "No layout to restore")))
