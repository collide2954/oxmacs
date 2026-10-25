(module
  (import (ox string) (ox time) (ox regex))
  (export parse property properties components unescape escape moment duration
          stamp date-stamp event-of write-event write-calendar fold-line))

(def (unfold text)
  (reduce (fn [out line]
            (cond ((and (not (empty? out)) (or (string/starts-with? line " ") (string/starts-with? line "\t")))
                   (conj (pop out) (str (peek out) (string/slice line 1))))
              ((string/blank? line) out)
              (else (conj out line))))
          [] (string/lines (string/replace text "\r" ""))))

(def (content-line line)
  (let [head (or (get (regex/find "^(?:\"[^\"]*\"|[^\":])*" line) :text) "")
        value (string/slice line (min (count line) (inc (count head))))
        parts (map :text (regex/find-all "(?:\"[^\"]*\"|[^\";])+" head))
        params (reduce (fn [params part]
                         (let [[key given] (string/split part "=" 2)]
                           (assoc params (string/upper key) (string/replace (or given "") "\"" ""))))
                       {} (rest parts))]
    {name: (string/upper (or (first parts) "")) params: params value: value}))

(def (parse text)
  "iCalendar text as nested components, each a map of :type, :properties and :children; properties are maps of :name, :params and :value."
  (let [step (fn [stack line]
               (let [content (content-line line)]
                 (cond ((= content.name "BEGIN")
                        (conj stack {type: (string/upper content.value) properties: [] children: []}))
                   ((= content.name "END")
                    (if (> (count stack) 1)
                      (let [done (peek stack)
                            below (pop stack)]
                        (conj (pop below) (update (peek below) :children conj done)))
                      stack))
                   (else (conj (pop stack) (update (peek stack) :properties conj content))))))]
    (first (reduce step [{type: "ROOT" properties: [] children: []}] (unfold text)))))

(def (property component name)
  "The first property of component called name, or nil."
  (find (fn [found] (= found.name name)) component.properties))

(def (properties component name)
  "Every property of component called name."
  (vec (filter (fn [found] (= found.name name)) component.properties)))

(def (components component kind)
  "Every component of kind, such as VEVENT, anywhere inside component."
  (vec (mapcat (fn [child]
                 (concat (if (= child.type kind) [child] []) (components child kind)))
               component.children)))

(def (unescape text)
  "Property text with its iCalendar escapes undone."
  (-> (or text "")
      (string/replace "\\\\" "\u{0}")
      (string/replace "\\n" "\n")
      (string/replace "\\N" "\n")
      (string/replace "\\," ",")
      (string/replace "\\;" ";")
      (string/replace "\u{0}" "\\")))

(def (escape text)
  "text written as iCalendar property text."
  (-> (or text "")
      (string/replace "\\" "\\\\")
      (string/replace ";" "\\;")
      (string/replace "," "\\,")
      (string/replace "\n" "\\n")))

(def (number-in text start end)
  (or (string/to-number (string/slice text start end)) 0))

(def (date-of value)
  {year: (number-in value 0 4) month: (number-in value 4 6) day: (number-in value 6 8)})

(def (moment content)
  "When a DTSTART-like property says, as a map of :time in milliseconds, :all-day, :date for all-day ones and the :zone it was given in."
  (let [value (string/trim (or content.value ""))
        all-day (or (= (get content.params "VALUE") "DATE") (= (count value) 8))
        date (date-of value)]
    (if all-day
      {time: (time/instant date) all-day: true date: date zone: nil}
      (let [fields (merge date {hour: (number-in value 9 11) minute: (number-in value 11 13) second: (number-in value 13 15)})
            zone (get content.params "TZID")
            time (cond ((string/ends-with? value "Z") (time/instant fields :zone "UTC"))
                   (zone (try (time/instant fields :zone zone) (catch error _ (time/instant fields))))
                   (else (time/instant fields)))]
        {time: time all-day: false date: nil zone: (when (and zone (not (string/ends-with? value "Z"))) zone)}))))

(def (duration text)
  "An iCalendar duration such as PT1H30M or -P1D in milliseconds."
  (let [given (string/trim (or text ""))
        negative (string/starts-with? given "-")
        match (regex/find #"P(?:(\d+)W)?(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?)?" given)
        part (fn [index] (or (string/to-number (or (get (get match :groups) index) "")) 0))
        total (+ (* (part 0) 604800000) (* (part 1) 86400000) (* (part 2) 3600000) (* (part 3) 60000) (* (part 4) 1000))]
    (if negative (- total) total)))

(def (stamp millis)
  "millis as an iCalendar UTC date and time."
  (time/format millis "%Y%m%dT%H%M%SZ" :zone "UTC"))

(def (date-stamp date)
  (str (string/pad-start (str date.year) 4 "0") (string/pad-start (str date.month) 2 "0") (string/pad-start (str date.day) 2 "0")))

(def (alarm-of component)
  (when-let [trigger (property component "TRIGGER")]
    (if (= (get trigger.params "VALUE") "DATE-TIME")
      {at: (get (moment trigger) :time)}
      {before: (- (duration trigger.value))})))

(def (attendee-of found)
  {email: (string/replace (string/replace found.value "mailto:" "") "MAILTO:" "")
   name: (get found.params "CN")
   status: (or (get found.params "PARTSTAT") "NEEDS-ACTION")})

(def (event-of component)
  "The event a VEVENT component describes, as a map of :uid, :summary, :start, :end, :all-day, :zone, :location, :description, :rrule, :exdates, :recurrence-id, :alarms, :organizer, :attendees and :status."
  (let [start (moment (or (property component "DTSTART") {value: "19700101" params: {}}))
        ending (property component "DTEND")
        length (when-let [given (property component "DURATION")] (duration given.value))
        end-time (cond (ending (get (moment ending) :time))
                   (length (+ start.time length))
                   (start.all-day (time/add start.time {days: 1}))
                   (else start.time))
        text (fn [name] (when-let [found (property component name)] (unescape found.value)))]
    {uid: (text "UID")
     summary: (or (text "SUMMARY") "(no title)")
     start: start.time
     end: end-time
     all-day: start.all-day
     zone: start.zone
     location: (text "LOCATION")
     description: (text "DESCRIPTION")
     status: (text "STATUS")
     rrule: (when-let [found (property component "RRULE")] found.value)
     exdates: (vec (mapcat (fn [found]
                             (map (fn [value] (get (moment {value: value params: found.params}) :time))
                                  (string/split found.value ",")))
                           (properties component "EXDATE")))
     recurrence-id: (when-let [found (property component "RECURRENCE-ID")] (get (moment found) :time))
     sequence: (or (when-let [found (property component "SEQUENCE")] (string/to-number found.value)) 0)
     alarms: (vec (keep alarm-of (filter (fn [child] (= child.type "VALARM")) component.children)))
     organizer: (when-let [found (property component "ORGANIZER")] (attendee-of found))
     attendees: (vec (map attendee-of (properties component "ATTENDEE")))}))

(def (fold-line line)
  "line folded at 74 characters, as iCalendar wants."
  (if (<= (count line) 74)
    line
    (str (string/slice line 0 74) "\r\n " (fold-line (string/slice line 74)))))

(def (when-line name event at)
  (cond (event.all-day (str name ";VALUE=DATE:" (date-stamp (time/local at))))
    (event.zone (str name ";TZID=" event.zone ":" (time/format at "%Y%m%dT%H%M%S" :zone event.zone)))
    (else (str name ":" (stamp at)))))

(def (attendee-line kind person)
  (str kind (if person.name (str ";CN=" person.name) "") (if (= kind "ATTENDEE") (str ";PARTSTAT=" person.status) "")
       ":mailto:" person.email))

(def (event-lines event)
  (vec (filter some?
               (concat ["BEGIN:VEVENT"
                        (str "UID:" event.uid)
                        (str "DTSTAMP:" (stamp (time/now)))
                        (when-line "DTSTART" event event.start)
                        (when-line "DTEND" event event.end)
                        (str "SUMMARY:" (escape event.summary))
                        (when event.location (str "LOCATION:" (escape event.location)))
                        (when event.description (str "DESCRIPTION:" (escape event.description)))
                        (when event.status (str "STATUS:" event.status))
                        (when event.rrule (str "RRULE:" event.rrule))
                        (when (not (empty? (or event.exdates []))) (str "EXDATE:" (string/join "," (map stamp event.exdates))))
                        (when event.recurrence-id (when-line "RECURRENCE-ID" event event.recurrence-id))
                        (when (> (or event.sequence 0) 0) (str "SEQUENCE:" event.sequence))
                        (when event.organizer (attendee-line "ORGANIZER" event.organizer))]
                       (map (fn [person] (attendee-line "ATTENDEE" person)) (or event.attendees []))
                       (mapcat (fn [alarm]
                                 (if alarm.before
                                   ["BEGIN:VALARM" "ACTION:DISPLAY" (str "DESCRIPTION:" (escape event.summary))
                                    (str "TRIGGER:-PT" (// alarm.before 60000) "M") "END:VALARM"]
                                   []))
                               (or event.alarms []))
                       ["END:VEVENT"]))))

(def (write-calendar events {method nil})
  "A whole iCalendar file holding events."
  (str (string/join "\r\n"
                    (map fold-line
                         (concat ["BEGIN:VCALENDAR" "VERSION:2.0" "PRODID:-//oxmacs//calendar//EN"]
                                 (if method [(str "METHOD:" method)] [])
                                 (mapcat event-lines events)
                                 ["END:VCALENDAR"])))
       "\r\n"))

(def (write-event event)
  "An iCalendar file holding one event."
  (write-calendar [event]))

(def sample-calendar
  "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:standup@example.com\r\nDTSTART;TZID=Europe/Paris:20261019T090000\r\nDTEND;TZID=Europe/Paris:20261019T091500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nEXDATE;TZID=Europe/Paris:20261021T090000\r\nSUMMARY:Stand\r\n up\\, daily\r\nLOCATION:Room 1\\; east\r\nDESCRIPTION:Line one\\nLine two\r\nORGANIZER;CN=Bo:mailto:bo@example.com\r\nATTENDEE;CN=Ana;PARTSTAT=NEEDS-ACTION:mailto:ana@example.com\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:trip@example.com\r\nDTSTART;VALUE=DATE:20261023\r\nDTEND;VALUE=DATE:20261025\r\nSUMMARY:Trip\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")

(test "icalendar files are read with folding, escapes, zones and alarms"
  (let [[standup trip] (map event-of (components (parse sample-calendar) "VEVENT"))]
    (expect (= standup.summary "Standup, daily"))
    (expect (= standup.location "Room 1; east"))
    (expect (= standup.description "Line one\nLine two"))
    (expect (= standup.start (time/instant "2026-10-19T07:00:00Z")))
    (expect (= (- standup.end standup.start) 900000))
    (expect (= standup.zone "Europe/Paris"))
    (expect (= standup.rrule "FREQ=WEEKLY;BYDAY=MO,WE"))
    (expect (= (count standup.exdates) 1))
    (expect (= standup.alarms [{before: 900000}]))
    (expect (= standup.organizer.email "bo@example.com"))
    (expect (= (get (first standup.attendees) :status) "NEEDS-ACTION"))
    (expect trip.all-day)
    (expect (= (- trip.end trip.start) (time/days 2)))))

(test "events write back as the same event"
  (let [events (map event-of (components (parse sample-calendar) "VEVENT"))
        again (map event-of (components (parse (write-calendar events)) "VEVENT"))
        fields (fn [event]
                 (select-keys event [:uid :summary :start :end :all-day :zone :location :description
                                     :rrule :exdates :alarms :organizer :attendees]))]
    (expect (= (map fields events) (map fields again)))))
