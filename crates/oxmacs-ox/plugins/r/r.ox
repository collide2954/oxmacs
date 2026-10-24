(module
  (import (ox string)
          (r session :as session)
          (r help :as help)
          (r pane :as pane)
          (r plots :as plots)
          (r data :as data)
          (r complete :as complete)
          (r definition :as definition)
          (r layout)
          (r package :as package)
          (r markdown :as markdown)))

(def console-keys
  {"RET" 'comint-send-input
   "M-p" 'comint-previous-input
   "M-n" 'comint-next-input
   "C-c C-z" 'r-switch-to-console
   "C-c C-v" 'r-help
   "C-c C-e" 'r-environment
   "C-c C-o" 'r-peek
   "M-." 'r-find-definition
   "TAB" 'r-complete
   "C-M-i" 'r-complete})

(session/on-marker "message" (fn [_ payload] (message payload)))
(session/on-marker "help" pane/show-help)
(session/on-marker "plot" plots/arrived)
(session/on-marker "gallery" plots/thumbnails)
(session/on-marker "zoom" plots/zoomed)
(session/on-marker "environment" data/listed)
(session/on-marker "table" data/tabled)
(session/on-marker "object" data/described)
(session/on-marker "stats" data/described-column)
(session/on-marker "peek" data/peeked)
(session/on-marker "packages" package/available)
(session/on-marker "tests" package/tested)
(session/on-marker "installed" package/installed)
(session/on-marker "rendered" markdown/rendered)
(on-saved :r-markdown markdown/saved)
(session/on-ready :environment data/environment-due)
(session/on-marker "complete" complete/answered)
(session/on-marker "signature" complete/signed)
(session/on-marker "definition" definition/answered)
(session/on-ready :signatures complete/forget-signatures)
(session/on-ready :asked complete/settled)
(on-idle :r-complete 0.25 (fn [] (when (just-typed?) (complete/typed))))
(on-idle :r-signature 0.4 complete/signature-due)

(def (insert-operator operator)
  (let [at (point)
        before (if (> at 0) (text-range (- at 1) at) "")
        spaced-after (= (text-range at (+ at 1)) " ")]
    (when (insert (str (if (string/blank? before) "" " ") operator (if spaced-after "" " ")))
      (when spaced-after
        (set-point (+ (point) 1))))))

(def (send-region)
  (when-let [[start end] (region)]
    (let [code (text-range start end)]
      (deactivate-mark)
      (session/send code)
      true)))

(def (send-and-step found)
  (if found
    (let [[start end] found
          code (text-range start end)]
      (set-point (or (statement-after end) (count (buffer-text))))
      (session/send code))
    (message "No R code here")))

(def (paragraph text at)
  (let [lines (string/split text "\n")
        total (count lines)
        starts (reduce (fn [found line] (conj found (+ (last found) (count line) 1))) [0] lines)
        here (or (last (filter (fn [line] (<= (nth starts line) at)) (range total))) 0)
        blank? (fn [line] (string/blank? (nth lines line)))
        start (find (fn [line] (not (blank? line))) (range here total))]
    (when start
      (let [first-line ((fn up [line] (if (and (> line 0) (not (blank? (- line 1)))) (up (- line 1)) line)) start)
            last-line ((fn down [line] (if (and (< (+ line 1) total) (not (blank? (+ line 1)))) (down (+ line 1)) line)) start)]
        [(nth starts first-line) (+ (nth starts last-line) (count (nth lines last-line)))]))))

(defcommand run-r "Start an R console beside the script"
  (when-let [running (session/ensure)]
    (session/show-console running true)))

(defcommand r-new-session "Start another R session beside the current one"
  (when-let [started (session/launch nil)]
    (session/link! started)
    (session/show-console started true)))

(defcommand r-send-statement "Send the statement or selection to R and step"
  (unless (send-region)
    (send-and-step (statement-at (point)))))

(defcommand r-send-function "Send the function or paragraph to R and step"
  (unless (send-region)
    (let [at (point)]
      (send-and-step (or (defun-around at) (paragraph (buffer-text) at))))))

(defcommand r-switch-to-console "Switch between the R script and its R session"
  (let [here (buffer-name)]
    (if (session/known? here)
      (let [scripts (filter session/script? (buffer-list))
            script (or (find (fn [name] (= (session/linked name) here)) scripts) (first scripts))]
        (if script
          (display-buffer script :select true)
          (message "No R script to go back to")))
      (when-let [running (session/ensure)]
        (session/link! running)
        (session/show-console running true)))))

(defcommand r-help "Show R's help on a topic in *R help*"
  (let [default (help/symbol-at-point)
        typed (string/trim (await (prompt (if default "R help on (default {default}): " "R help on: "))))
        topic (if (string/blank? typed) default typed)]
    (when topic
      (pane/ask-help topic))))

(defcommand r-complete "Complete the R code before point from the session"
  (complete/complete true))

(defcommand r-indent-or-complete "Indent the line, or complete the R code once it is indented"
  (complete/indent-or-complete))

(defcommand r-find-definition "Jump to the definition of the R name at point"
  (definition/find-definition))

(defcommand r-insert-assignment "Insert the assignment arrow <-"
  (insert-operator "<-"))

(defcommand r-insert-pipe "Insert the native pipe |>"
  (insert-operator "|>"))

(session/use-keys! console-keys)
