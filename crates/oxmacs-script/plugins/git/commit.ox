(module
  (import (ox string))
  (export template message-text replace-message highlight))

(def summary-limit 50)

(def (template message snapshot)
  (let [lines (concat [(string/trim-end message) ""
                       "# Write the message above; lines starting with # are left out."
                       "# C-c C-c commits and C-c C-k cancels."]
                      (if snapshot.branch ["#" $"# On branch {snapshot.branch}"] [])
                      (if (empty? snapshot.staged)
                        []
                        (cons "# Changes to be committed:"
                              (map (fn [change] $"#   {change.path}") snapshot.staged))))]
    (str (string/join "\n" lines) "\n")))

(def (message-text text)
  (string/trim (string/join "\n" (filter (fn [line] (not (string/starts-with? line "#")))
                                         (string/split text "\n")))))

(def (replace-message text message)
  (let [at (string/index-of text "\n#")
        rest (cond
               [(string/starts-with? text "#") text]
               [(nil? at) ""]
               [else (string/slice text (+ at 1))])]
    (str (string/trim-end message) "\n\n" rest)))

(def trailer-chars "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-")

(def (trailer text)
  (let [at (string/index-of text ": ")]
    (if (and at (> at 0)
             (every? (fn [character] (string/includes? trailer-chars (str character)))
                     (string/chars (string/slice text 0 at))))
      [[0 (+ at 1) :keyword]]
      [])))

(def (highlight line text)
  (let [width (string/length text)]
    (cond
      [(string/starts-with? text "#") [[0 width :comment]]]
      [(= line 0)
       (cond
         [(> width summary-limit) [[0 summary-limit :heading] [summary-limit width :warning]]]
         [(> width 0) [[0 width :heading]]]
         [else []])]
      [(= line 1) (if (> width 0) [[0 width :error]] [])]
      [else (trailer text)])))
