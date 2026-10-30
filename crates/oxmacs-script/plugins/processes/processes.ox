(module
  (import (processes table :as table) (ox time)))

(def buffer "*Processes*")

(defonce state (atom {sort: :started descending: false filter: "" open: #{}}))

(def process-keys
  {"RET" 'process-visit "TAB" 'process-details "k" 'process-stop "K" 'process-kill
   "s" 'process-sort "S" 'process-sort-reverse "/" 'process-filter
   "g" 'process-refresh "n" 'next-line "p" 'previous-line "q" 'process-quit})

(def (draw opening)
  (let [shown (table/rows (processes) (time/now) (or (buffer-width buffer) 120) @state)]
    (if opening
      (open-buffer buffer shown.lines :keys process-keys :beside true :select true :on-resize (fn [_] (draw false)))
      (set-lines buffer shown.lines))
    (set-header buffer shown.header)))

(def (refresh)
  (if (buffer-exists? buffer)
    (do (watch-process-usage (buffer-shown? buffer))
      (draw false))
    (watch-process-usage false)))

(on-processes-change :processes refresh)

(def (process-here)
  (get (line-item) :id))

(def (on-process act)
  (if-let [id (process-here)]
    (try (act id) (catch error e (message e.message)))
    (message "No process on this line")))

(def (named id)
  (or (some (fn [p] (when (= p.id id) p.name)) (processes)) "the process"))

(defcommand list-processes "List the processes oxmacs runs"
  (watch-process-usage true)
  (draw true))

(defcommand process-visit "Show the buffer of the process on this line"
  (on-process visit-process))

(defcommand process-details "Show or hide the command, folder, status and environment of the process on this line"
  (on-process (fn [id]
                (swap! state update :open (fn [open] (if (contains? open id) (disj open id) (conj open id))))
                (draw false))))

(defcommand process-stop "Ask the process on this line to end"
  (on-process (fn [id]
                (when (await (yes-or-no $"Stop {(named id)}?"))
                  (stop-process id)))))

(defcommand process-kill "End the process on this line at once"
  (on-process (fn [id]
                (when (await (yes-or-no $"Kill {(named id)}?"))
                  (kill-process id)))))

(defcommand process-sort "Sort the processes by the next column"
  (swap! state update :sort table/next-sort)
  (draw false)
  (message $"Sorted by {(name (get @state :sort))}"))

(defcommand process-sort-reverse "Reverse the order of the processes"
  (swap! state update :descending not)
  (draw false))

(defcommand process-filter "Show only processes whose name, kind or command contains some text"
  (let [query (await (prompt "Filter processes: " :initial (get @state :filter)))]
    (swap! state assoc :filter query)
    (draw false)))

(defcommand process-refresh "Read the processes again"
  (draw false))

(defcommand process-quit "Close the process list"
  (watch-process-usage false)
  (run-command 'quit-window))
