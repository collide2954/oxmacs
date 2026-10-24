(module
  (import (ox string) (ox regex)
          (r session :as session)
          (r language :as language))
  (export in-r? complete answered indent-or-complete typed call-name signature-due
          signed forget-signatures settled))

(def auto-prefix 3)
(def window 4000)
(def keywords #{"if" "for" "while" "function"})

(defonce asked (atom nil))
(defonce signatures (atom {shown: nil known: {}}))

(def (in-r?)
  (or (= (buffer-language) "R") (session/known? (buffer-name))))

(def (line-start) (- (point) (current-column)))

(def (complete explicit)
  (when (in-r?)
    (let [running (session/current-session)
          refusal (cond ((not (session/running? running)) "Start R with C-c C-z to complete from its session")
                    ((not (console-waiting? running)) "R is busy")
                    (else nil))]
      (cond
        ((and refusal explicit (language/ready?)) (run-command 'completion-at-point))
        (refusal (when explicit (message refusal)))
        (else
         (let [start (max (line-start) (or (console-input-start) 0))
               line (text-between start (point))]
           (when (session/request running "getOption(\"oxmacs\")$complete({(session/r-string line)}, {(count line)}L)")
             (reset! asked {buffer: (buffer-name) start: start explicit: explicit}))))))))

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
        (complete true)))))

(def (typed-prefix)
  (let [line (text-between (line-start) (point))]
    (if-let [found (regex/find #"[\p{L}\p{N}._]+$" line)]
      (count found.text)
      0)))

(def (in-comment? line)
  ((fn scan [characters quote]
     (cond ((empty? characters) false)
       (quote (scan (rest characters) (if (= (first characters) quote) nil quote)))
       ((contains? #{\" \' \`} (first characters)) (scan (rest characters) (first characters)))
       ((= (first characters) \#) true)
       (else (scan (rest characters) nil))))
   (string/chars line) nil))

(def (typed)
  (when (and (in-r?) (not @asked) (not (completion-shown?)) (not (language/ready?)))
    (let [running (session/current-session)]
      (when (and (session/running? running)
                 (console-waiting? running)
                 (>= (typed-prefix) auto-prefix)
                 (not (in-comment? (text-between (line-start) (point)))))
        (complete false)))))

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

(def (signature-due)
  (let [running (session/current-session)]
    (when (and (session/running? running) (not (prompting?)) (not (completion-shown?)) (not (language/ready?)))
      (let [spot (call-at-point)
            name (when spot (nth spot 2))]
        (cond ((not spot) (swap! signatures assoc :shown nil))
          ((= spot (get @signatures :shown)) nil)
          ((contains? (get @signatures :known) name)
           (let [known (get-in @signatures [:known name])]
             (swap! signatures assoc :shown spot)
             (unless (string/blank? known) (message known))))
          ((and (console-waiting? running) (not @asked))
           (swap! signatures assoc :shown spot)
           (session/request running "getOption(\"oxmacs\")$signature({(session/r-string name)})"))
          (else nil))))))

(def (signed _ payload)
  (let [[name text] (string/split payload "\t" 2)
        text (or text "")]
    (swap! signatures assoc-in [:known name] text)
    (when (and (= (when-let [spot (call-at-point)] (nth spot 2)) name) (not (string/blank? text)))
      (message text))))
