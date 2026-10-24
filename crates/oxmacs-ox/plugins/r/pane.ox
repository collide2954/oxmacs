(module
  (import (ox string)
          (r session :as session)
          (r help :as help))
  (export show-help ask-help))

(def help-buffer "*R help*")

(defonce state (atom {history: [] at: -1 pending: nil page: nil}))

(def help-keys
  {"SPC" 'scroll-up-command "DEL" 'scroll-down-command "q" 'quit-window
   "RET" 'r-help-follow "TAB" 'r-help-next-link "S-TAB" 'r-help-previous-link
   "l" 'r-help-back "r" 'r-help-forward "x" 'r-help-run-example "e" 'r-help-run-examples})

(def (render text)
  (let [shown (help/page text)]
    (swap! state assoc :page shown)
    (open-buffer help-buffer shown.lines :keys help-keys :beside true :select true :line 0)))

(def (show-help _ text)
  (let [known @state
        topic (or known.pending (help/topic-of text))
        current (get known.history known.at)
        again (and current (= current.topic topic))
        history (if again
                  (assoc known.history known.at {topic: topic text: text})
                  (conj (vec (take (+ known.at 1) known.history)) {topic: topic text: text}))]
    (swap! state assoc :history history :at (if again known.at (- (count history) 1)) :pending nil)
    (render text)))

(def (ask-help topic)
  (when-let [running (session/ensure)]
    (swap! state assoc :pending topic)
    (session/request running "getOption(\"oxmacs\")$help({(session/r-string topic)})")))

(def (move step edge)
  (let [known @state
        at (+ known.at step)]
    (if (and (>= at 0) (< at (count known.history)))
      (do (swap! state assoc :at at)
        (render (get-in known [:history at :text])))
      (message "No {edge} help page"))))

(def (shown-page) (when (= (buffer-name) help-buffer) (get @state :page)))

(def (send code)
  (when-let [running (session/ensure)]
    (session/show-console running false)
    (session/evaluate running code)))

(defcommand r-help-follow "Show the help page the link at point names"
  (if-let [link (when-let [page (shown-page)] (help/link-at page (current-line) (current-column)))]
    (ask-help link.topic)
    (message "No link here")))

(def (jump forward)
  (when-let [page (shown-page)]
    (if-let [link (help/next-link page (current-line) (current-column) forward)]
      (goto-line link.line link.start)
      (message "No more links"))))

(defcommand r-help-next-link "Move to the next link in R's help" (jump true))

(defcommand r-help-previous-link "Move to the previous link in R's help" (jump false))

(defcommand r-help-back "Show the help page seen before this one" (move -1 "earlier"))

(defcommand r-help-forward "Show the help page seen after this one" (move 1 "later"))

(defcommand r-help-run-example "Run the example at point in the R console"
  (if-let [code (when-let [page (shown-page)] (help/example-at page (current-line)))]
    (send code)
    (message "No example here")))

(defcommand r-help-run-examples "Run every example of the help page in the R console"
  (if-let [code (when-let [page (shown-page)] (help/examples page))]
    (send code)
    (message "This page has no examples")))
