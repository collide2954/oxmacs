(module
  (import (ox string) (ox regex))
  (export page symbol-in))

(def backspace \u{8})

(def (runs line)
  "The text of a line of R's help as [text struck] runs, where struck text was overstruck to show it as a heading."
  (let [chars (string/chars line)
        total (count chars)]
    ((fn step [at found]
       (if (>= at total)
         found
         (let [struck (and (< (+ at 2) total) (= (nth chars (+ at 1)) backspace))
               shown (if struck (nth chars (+ at 2)) (nth chars at))
               next (if struck (+ at 3) (+ at 1))]
           (cond ((= shown backspace) (step next found))
                 ((and (not (empty? found)) (= (second (last found)) struck))
                  (step next (conj (pop found) [(str (first (last found)) shown) struck])))
                 (else (step next (conj found [(str shown) struck])))))))
     0 [])))

(def (page text)
  "R's help text as lines for open-buffer, with its headings in the heading face."
  (map (fn [line]
         (map (fn [[shown struck]] (if struck [shown :heading] shown)) (runs line)))
       (string/lines text)))

(def (symbol-in line column)
  "The R name in line around column, or nil."
  (when-let [found (find (fn [word] (and (<= word.start column) (<= column word.end)))
                         (regex/find-all #"[\p{L}\p{N}._]+" line))]
    (when (regex/matches? #"^[^0-9_]" found.text)
      found.text)))
