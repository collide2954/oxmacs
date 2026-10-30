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
          (r markdown :as markdown)
          (r language :as language)
          (r jobs)
          (r debug :as debug)))

(def console-keys
  {"RET" 'comint-send-input
   "M-p" 'comint-previous-input
   "M-n" 'comint-next-input
   "C-c C-z" 'r-switch-to-console
   "C-c C-v" 'r-help
   "C-c C-e" 'r-environment
   "C-c C-o" 'r-peek
   "TAB" 'completion-at-point
   "<f10>" 'r-debug-next
   "S-<f4>" 'r-debug-step
   "S-<f6>" 'r-debug-finish
   "S-<f5>" 'r-debug-continue
   "S-<f8>" 'r-debug-stop})

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
(session/on-marker "viewer" (fn [_ url] (open-externally url) (message $"Opened {url}")))
(on-saved :r-markdown markdown/saved)
(session/on-ready :environment data/environment-due)
(session/on-marker "complete" complete/answered)
(session/on-marker "signature" complete/signed)
(session/on-marker "definition" definition/answered)
(session/on-ready :signatures complete/forget-signatures)
(session/on-ready :asked complete/settled)

(def hooks
  {complete: complete/complete signature: complete/signature definition: definition/find-definition})

(defmode r "R" :complete hooks.complete :signature hooks.signature :definition hooks.definition)
(session/use-hooks! hooks)

(def (insert-operator operator)
  (let [at (point)
        before (if (> at 0) (text-between (- at 1) at) "")
        spaced-after (= (text-between at (+ at 1)) " ")]
    (try
      (insert (str (if (string/blank? before) "" " ") operator (if spaced-after "" " ")))
      (when spaced-after
        (set-point (+ (point) 1)))
      (catch error e (message e.message)))))

(def (send-region)
  (when-let [[start end] (region)]
    (let [code (text-between start end)]
      (deactivate-mark)
      (session/send code)
      true)))

(def (send-and-step found)
  (if found
    (let [[start end] found
          code (text-between start end)]
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
        typed (string/trim (await (prompt (if default $"R help on (default {default}): " "R help on: "))))
        topic (if (string/blank? typed) default typed)]
    (when topic
      (pane/ask-help topic))))

(defcommand r-indent-or-complete "Indent the line, or complete the R code once it is indented"
  (complete/indent-or-complete))

(defcommand r-insert-assignment "Insert the assignment arrow <-"
  (insert-operator "<-"))

(defcommand r-insert-pipe "Insert the native pipe |>"
  (insert-operator "|>"))

(session/use-keys! console-keys)
(session/use-patterns! debug/prompts debug/watches)
(session/on-marker "debug" debug/heard)

(spawn (language/check))
