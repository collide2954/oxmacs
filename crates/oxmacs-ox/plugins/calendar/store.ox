(module
  (import (ox file) (ox path) (ox string) (ox crypto) (ox json)
          (calendar ics :as ics)
          (calendar rrule :as rrule))
  (export folder calendars calendar-of calendar-folder load! events occurrences save-event! delete-event!
          skip-occurrence! new-uid safe sync-file read-sync write-sync!))

(defonce state (atom {events: []}))

(def (folder)
  "Where calendars are kept: calendar in the oxmacs home folder."
  (when-let [home (oxmacs-home)]
    (path/join home "calendar")))

(def (calendars)
  "The calendars in use, each a map with at least a :name; Personal when none are set."
  (let [given (or (plugin-setting 'calendar :calendars) [])]
    (if (empty? given) [{name: "Personal"}] given)))

(def (calendar-of name)
  (find (fn [calendar] (= calendar.name name)) (calendars)))

(def (safe text)
  (reduce (fn [out c] (string/replace out c "_")) text ["/" "\\" ":" "*" "?" "\"" "<" ">" "|" "@"]))

(def (calendar-folder name)
  (path/join (or (folder) ".") (safe name)))

(def (sync-file name)
  (path/join (calendar-folder name) ".sync.json"))

(def (read-sync name)
  "What the last CalDAV sync of calendar name knew: its :resources, each a map of :href, :etag and :file, and the :pending local changes."
  (let [file (sync-file name)]
    (if (await (file/exists? file))
      (let [found (json/read (await (file/read file)))]
        {resources: (vec (or (get found :resources) [])) pending: (vec (or (get found :pending) []))})
      {resources: [] pending: []})))

(def (write-sync! name sync)
  (await (file/make-dir (calendar-folder name)))
  (await (file/write (sync-file name) (json/write sync))))

(def (read-file calendar file)
  (let [parsed (ics/parse (await (file/read file)))]
    (vec (map (fn [component] (merge (ics/event-of component) {calendar: calendar file: file}))
              (ics/components parsed "VEVENT")))))

(def (load!)
  "Reads the .ics files of every calendar again and returns their events."
  (let [found (vec (mapcat (fn [calendar]
                             (let [place (calendar-folder calendar.name)]
                               (if (await (file/exists? place))
                                 (mapcat (fn [entry]
                                           (try (read-file calendar.name entry.path)
                                             (catch error e
                                               (message (str "Couldn't read " entry.name ": " e.message))
                                               [])))
                                         (filter (fn [entry] (string/ends-with? (string/lower entry.name) ".ics"))
                                                 (await (file/list place))))
                                 [])))
                           (calendars)))]
    (swap! state assoc :events found)
    found))

(def (events)
  (get @state :events))

(def (occurrences from to)
  "Every occurrence that overlaps from up to to, sorted by start: the event's fields with that occurrence's :start and :end, and the :event itself."
  (let [all (events)
        replaced (set (keep (fn [event] (when event.recurrence-id [event.uid event.recurrence-id])) all))]
    (vec (sort-by :start
                  (mapcat (fn [event]
                            (keep (fn [found]
                                    (when (or event.recurrence-id (not (contains? replaced [event.uid found.start])))
                                      (merge event found {event: event})))
                                  (rrule/expand (if event.recurrence-id (dissoc event :rrule) event) from to)))
                          all)))))

(def (new-uid)
  (str (crypto/random 12) "@oxmacs"))

(def (same? a b)
  (and (= a.uid b.uid) (= a.recurrence-id b.recurrence-id)))

(def (note-change! calendar file removed)
  (when (get (calendar-of calendar) :url)
    (let [sync (read-sync calendar)
          name (path/name file)
          pending (vec (remove (fn [change] (= change.file name)) sync.pending))]
      (write-sync! calendar (assoc sync :pending (conj pending {file: name removed: removed}))))))

(def (save-event! event)
  "Writes event into its file, or a new file in its calendar, keeping the other events of that file; returns the file."
  (let [file (or event.file (path/join (calendar-folder event.calendar) (str (safe event.uid) ".ics")))
        others (filter (fn [other] (and (= other.file file) (not (same? other event)))) (events))]
    (await (file/make-dir (path/parent file)))
    (await (file/write file (ics/write-calendar (conj (vec others) (dissoc event :file :calendar :event)))))
    (note-change! event.calendar file false)
    (load!)
    file))

(def (delete-event! event)
  "Removes event and the changed occurrences of its series, moving its file to the trash when nothing is left in it."
  (let [file event.file
        kept (vec (filter (fn [other] (and (= other.file file) (not= other.uid event.uid))) (events)))]
    (if (empty? kept)
      (await (file/trash file))
      (await (file/write file (ics/write-calendar kept))))
    (note-change! event.calendar file (empty? kept))
    (load!)))

(def (skip-occurrence! event start)
  "Leaves the occurrence of a repeating event at start out of its series."
  (let [master (or (find (fn [other] (and (= other.uid event.uid) (nil? other.recurrence-id))) (events)) event)]
    (save-event! (update master :exdates (fn [dates] (conj (vec (or dates [])) start))))))
