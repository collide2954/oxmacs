(module
  (import (ox string) (ox regex)
          (r session :as session)
          (r language :as language))
  (export complete answered indent-or-complete call-name signature signed forget-signatures
          settled))

(def window 4000)
(def keywords #{"if" "for" "while" "function"})

(defonce asked (atom nil))
(defonce signatures (atom {shown: nil known: {}}))

(def (in-r?)
  (or (= (buffer-language) "R") (session/known? (buffer-name))))

(def ^:shadow (line-start) (- (point) (current-column)))

(def (complete explicit)
  (let [running (session/current-session)
        refusal (cond ((not (session/running? running)) "Start R with C-c C-z to complete from its session")
                  ((not (console-waiting? running)) "R is busy")
                  (else nil))]
    (cond
      ((and (not explicit) (or refusal @asked (language/ready?))) nil)
      ((and refusal (language/ready?)) nil)
      (refusal (message refusal) true)
      (else
       (let [start (max (line-start) (or (console-input-start) 0))
             line (text-between start (point))]
         (when (session/request running "getOption(\"oxmacs\")$complete({(session/r-string line)}, {(count line)}L)")
           (reset! asked {buffer: (buffer-name) start: start explicit: explicit}))
         true)))))

(def (answered _ payload)
  (when-let [question @asked]
    (reset! asked nil)
    (let [fields (string/split payload "\t")
          offset (string/to-number (first fields))]
      (when offset
        (let [start (+ question.start offset)]
          (when (and (not (prompting?))
                     (= (buffer-name) question.buffer)
                     (<= start (point))
                     (not (string/includes? (text-between start (point)) "\n")))
            (complete-at start (vec (rest fields)) question.explicit)))))))

(def (indent-or-complete)
  (let [before [(point) (line-text (current-line))]
        selecting (region)]
    (run-command 'indent-for-tab-command)
    (let [at (point)
          after-code (and (> at 0) (not (string/blank? (text-between (- at 1) at))))]
      (when (and (= before [at (line-text (current-line))]) (not selecting) after-code)
        (run-command 'completion-at-point)))))

(def (call-name text)
  (let [characters (string/chars text)
        total (count characters)
        opened ((fn scan [at open quote escaped comment]
                  (if (>= at total)
                    (when (and (not quote) (not comment)) open)
                    (let [character (nth characters at)]
                      (cond (comment (scan (+ at 1) open quote false (not= character \newline)))
                        (quote (cond (escaped (scan (+ at 1) open quote false false))
                                 ((= character \\) (scan (+ at 1) open quote true false))
                                 ((= character quote) (scan (+ at 1) open nil false false))
                                 (else (scan (+ at 1) open quote false false))))
                        ((contains? #{\" \' \`} character) (scan (+ at 1) open character false false))
                        ((= character \#) (scan (+ at 1) open nil false true))
                        ((contains? #{\( \[ \{} character) (scan (+ at 1) (conj open [character at]) nil false false))
                        ((contains? #{\) \] \}} character) (scan (+ at 1) (if (empty? open) open (pop open)) nil false false))
                        (else (scan (+ at 1) open nil false false))))))
                0 [] nil false false)]
    (when-let [[bracket at] (when (not (empty? opened)) (last opened))]
      (when (= bracket \()
        (let [before (string/trim-end (string/slice text 0 at))
              found (regex/find #"[\p{L}\p{N}._:]+$" before)
              name (if found found.text "")]
          (when (and (regex/matches? #"^[\p{L}.]" name)
                     (not (string/ends-with? name ":"))
                     (not (contains? keywords name)))
            [name at]))))))

(def (call-at-point)
  (when (in-r?)
    (let [at (point)
          floor (min at (or (console-input-start) 0))
          from (max floor (- at window))]
      (when-let [[name paren] (call-name (text-between from at))]
        [(buffer-name) (+ from paren) name]))))

(def (settled _) (reset! asked nil))

(def (forget-signatures _)
  (reset! signatures {shown: nil known: {}}))

(def (signature)
  (let [running (session/current-session)
        spot (call-at-point)
        name (when spot (nth spot 2))]
    (cond ((or (not spot) (language/ready?)) nil)
      ((contains? (get @signatures :known) name)
       (let [known (get-in @signatures [:known name])]
         (if (string/blank? known) nil known)))
      ((= spot (get @signatures :shown)) true)
      ((and (session/running? running) (console-waiting? running) (not @asked))
       (swap! signatures assoc :shown spot)
       (session/request running "getOption(\"oxmacs\")$signature({(session/r-string name)})")
       true)
      (else nil))))

(def (signed _ payload)
  (let [[name text] (string/split payload "\t" 2)
        text (or text "")]
    (swap! signatures assoc-in [:known name] text)
    (when (and (= (when-let [spot (call-at-point)] (nth spot 2)) name) (not (string/blank? text)))
      (message text))))
