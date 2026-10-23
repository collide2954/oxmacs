(module
  (import (ox time)
          (calendar store :as store))
  (export default-minutes alarm-times remind! watch!))

(defonce state (atom {reminded: (set []) checked: nil watching: false}))

(def (each f coll)
  (reduce (fn [_ item] (f item) nil) nil coll))

(def (default-minutes)
  "Minutes before an event to remind you when it sets no reminder of its own."
  (or (plugin-setting 'calendar :reminder) 10))

(def (alarm-times occurrence)
  "When to remind you of occurrence, in milliseconds."
  (let [alarms (if (empty? (or occurrence.alarms [])) [{before: (time/minutes (default-minutes))}] occurrence.alarms)]
    (map (fn [alarm] (if alarm.at alarm.at (- occurrence.start alarm.before))) alarms)))

(def (remind!)
  "Says which events start soon, once each, in the echo area and as a desktop notification."
  (let [now (time/now)
        since (or (get @state :checked) (- now (time/minutes 1)))
        found (store/occurrences (- now (time/hours 1)) (+ now (time/days 2)))]
    (swap! state assoc :checked now)
    (each (fn [occurrence]
            (each (fn [at]
                    (let [key [occurrence.uid occurrence.start at]]
                      (when (and (> at since) (<= at now) (not (contains? (get @state :reminded) key)))
                        (swap! state update :reminded conj key)
                        (let [text (str occurrence.summary
                                        (if occurrence.all-day " today" (str " at " (time/format occurrence.start "%H:%M")))
                                        (if occurrence.location (str ", " occurrence.location) ""))]
                          (message text)
                          (notify "Calendar" text)))))
                  (alarm-times occurrence)))
          found)))

(def (watch!)
  "Checks for reminders every half minute from now on."
  (unless (get @state :watching)
    (swap! state assoc :watching true)
    (every (time/seconds 30) (fn [] (remind!) nil))))
