(module
  (import (ox regex) (ox string))
  (export linger shown? push live rows caption wait))

(def linger 2000)
(def fade 400)
(def frame 50)
(def most 3)
(def faintness [1 0.6 0.35])

(def (chord? key)
  (regex/matches? "^(?:[CMSs]-)*[CMs]-." key))

(def (named? key)
  (regex/matches? "^(?:S-)?(?:RET|TAB|ESC|<(?:up|down|left|right|f[0-9]+)>)$" key))

(def (letter? key)
  (or (= key "SPC") (= (count key) 1)))

(def (shown? event)
  "True when the keys of event are worth showing: a chord with Ctrl, Alt or Super, RET, TAB, ESC, an arrow or a function key, or plain letters a special buffer binds."
  (let [keys event.keys]
    (or (some (fn [key] (or (chord? key) (named? key))) keys)
        (every? letter? keys))))

(def (same? cast event)
  (and (= cast.keys event.keys) (= cast.command event.command)))

(def (push casts event now)
  "casts with event pressed at now added at the end: pressing the newest again counts up, and only the newest three stay."
  (let [newest (last casts)]
    (if (and newest (same? newest event))
      (conj (pop casts) (assoc newest :count (inc newest.count) :at now))
      (let [all (conj casts {keys: event.keys command: event.command count: 1 at: now})]
        (vec (drop (max 0 (- (count all) most)) all))))))

(def (live casts now)
  "The casts pressed less than linger milliseconds before now."
  (vec (filter (fn [cast] (< (- now cast.at) linger)) casts)))

(def (opacity casts index now)
  (let [cast (nth casts index)
        left (- linger (- now cast.at))]
    (* (get faintness (- (count casts) index 1) (last faintness))
       (min 1 (max 0 (/ left fade))))))

(def (pieces cast)
  (concat (map (fn [key] [key :keycap]) cast.keys)
          (if (> cast.count 1) [["×{cast.count}" :comment]] [])
          [cast.command]))

(def (rows casts now)
  "Overlay rows for casts at now, oldest first and faintest, each fading out in its last moments."
  (vec (map (fn [index] {pieces: (pieces (nth casts index)) opacity: (opacity casts index now)})
            (range (count casts)))))

(def (caption cast)
  "The mode-line text for cast, or nil when there is none."
  (when cast
    (str (string/join " " (map (fn [key] "[{key}]") cast.keys))
         (if (> cast.count 1) " ×{cast.count}" "")
         " " cast.command)))

(def (wait casts now)
  "Milliseconds until casts next look different at now, or nil when there are none."
  (when (not (empty? casts))
    (let [fades (map (fn [cast] (- (+ cast.at linger) fade now)) casts)]
      (if (some (fn [left] (<= left 0)) fades)
        frame
        (apply min fades)))))
