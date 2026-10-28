(module
  (import (profiler report :as report)))

(def buffer "*ox profile*")

(defonce state (atom {found: nil folded: #{} flat: false}))

(def report-keys
  {"TAB" 'ox-profiler-toggle "f" 'ox-profiler-flat "RET" 'ox-profiler-visit
   "g" 'ox-profiler-refresh "q" 'quit-window})

(def (draw opening)
  (let [now @state
        report (report/rows now.found now.folded now.flat)]
    (if opening
      (open-buffer buffer report.lines :keys report-keys)
      (set-lines buffer report.lines))
    (set-header buffer report.header)))

(def (show found)
  (swap! state assoc :found found :folded #{})
  (draw true)
  (goto-item {}))

(def (row-here)
  (line-item))

(defcommand ox-profiler-start "Start profiling the Ox code oxmacs runs"
  (if (profiler-start)
    (message "Profiling Ox; M-x ox-profiler-stop shows where the time went")
    (message "The profiler is already running")))

(defcommand ox-profiler-stop "Stop profiling and show where the time went"
  (if-let [found (profiler-stop)]
    (show found)
    (message "The profiler isn't running")))

(defcommand ox-profiler-report "Show where the time went so far"
  (if-let [found (or (profiler-snapshot) (get @state :found))]
    (show found)
    (message "Start the profiler with M-x ox-profiler-start")))

(defcommand ox-profiler-refresh "Take a fresh report while profiling continues"
  (if-let [found (profiler-snapshot)]
    (do (swap! state assoc :found found)
      (draw false))
    (message "The profiler isn't running; this report is final")))

(defcommand ox-profiler-toggle "Fold or unfold the branch at point"
  (when-let [row (row-here)]
    (when row.path
      (swap! state update :folded
             (fn [folded] (if (contains? folded row.path) (disj folded row.path) (conj folded row.path))))
      (draw false))))

(defcommand ox-profiler-flat "Switch between the call tree and the functions by self time"
  (swap! state update :flat not)
  (draw false)
  (goto-item {}))

(defcommand ox-profiler-visit "Visit the source of the function at point"
  (when-let [row (row-here)]
    (if (and row.file (open-file row.file))
      (goto-line (- row.line-number 1))
      (message "This function has no source file"))))
