(module
  (import (ox string) (ox time) (ox file) (ox path) (ox ui) (ox regex)
          (calendar ics :as ics)
          (calendar store :as store)
          (calendar caldav :as caldav)
          (calendar reminders :as reminders)))

(def agenda-buffer "*Calendar*")
(def month-buffer "*Calendar month*")
(def week-buffer "*Calendar week*")
(def event-buffer "*Calendar event*")
(def first-row 3)
(def repeats {"never" nil "daily" "FREQ=DAILY" "weekdays" "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"
              "weekly" "FREQ=WEEKLY" "monthly" "FREQ=MONTHLY" "yearly" "FREQ=YEARLY"})
(def day-names ["Mon" "Tue" "Wed" "Thu" "Fri" "Sat" "Sun"])

(defonce view (atom {from: nil day: nil rows: [] shown: nil}))

(def (quietly work)
  (try (work)
    (catch error e
      (unless (= e.message "Quit")
        (message e.message)))))

(def (today)
  (let [now (time/local (time/now))]
    (time/day-number now)))

(def (day-start days)
  (time/instant (time/date-of days)))

(def (day-of millis)
  (let [local (time/local millis)]
    (time/day-number local)))

(def (clock millis)
  (time/format millis "%H:%M"))

(def (calendar-face name)
  (let [names (map :name (store/calendars))
        index (or (some (fn [[index known]] (when (= known name) index)) (zip (range (count names)) names)) 0)]
    (keyword (str "nick-" (mod index 8)))))

(def (span-text occurrence day)
  (let [starts (day-start day)
        ends (day-start (inc day))]
    (cond (occurrence.all-day "all day    ")
      ((and (< occurrence.start starts) (> occurrence.end ends)) "all day    ")
      ((< occurrence.start starts) (str "…–" (clock occurrence.end) "    "))
      ((> occurrence.end ends) (str (clock occurrence.start) "–…    "))
      (else (str (clock occurrence.start) "–" (clock occurrence.end))))))

(def (event-line occurrence day)
  {line: (vec (filter some?
                      ["  " [(span-text occurrence day) :comment] "  "
                       [occurrence.summary (calendar-face occurrence.calendar)]
                       (when occurrence.location [(str "  @ " occurrence.location) :comment])
                       (when occurrence.rrule ["  ↻" :comment])
                       (when (some (fn [person] (= person.status "NEEDS-ACTION")) (or occurrence.attendees [])) ["  invitation" :warning])
                       [(str "  " occurrence.calendar) :comment]]))})

(def (agenda from days)
  "The lines and rows of the agenda from the day numbered from, for days days."
  (let [found (store/occurrences (day-start from) (day-start (+ from days)))
        now (today)
        title (str (time/format (day-start from) "%a %-d %b") " – " (time/format (day-start (+ from days -1)) "%a %-d %b %Y"))
        top [{line: [["Calendar" :heading] (str "  " title)]}
             {line: [["RET shows  a adds  e edits  d deletes  f/b weeks  t today  m month  w week  i imports  s syncs  q quits" :comment]]}
             {line: []}]
        step (fn [[lines rows] day]
               (let [here (filter (fn [found] (and (< found.start (day-start (inc day)))
                                                   (> (max found.end (inc found.start)) (day-start day))))
                                  found)
                     here (vec (concat (filter :all-day here) (remove :all-day here)))]
                 (if (and (empty? here) (not= day now))
                   [lines rows]
                   (let [heading {line: [[(time/format (day-start day) "%A %-d %B") (if (= day now) :keyword :heading)]
                                         (when (= day now) ["  today" :comment])]}
                         body (if (empty? here)
                                [{line: [["  Nothing planned" :comment]]}]
                                (map (fn [found] (event-line found day)) here))]
                     [(vec (concat lines [heading] body [{line: []}]))
                      (vec (concat rows [nil] (if (empty? here) [nil] here) [nil]))]))))
        [lines rows] (reduce step [[] []] (range from (+ from days)))]
    {lines: (vec (concat top lines)) rows: (vec (concat [nil nil nil] rows))}))

(def (agenda-days)
  (or (plugin-setting 'calendar :days) 14))

(def (show-agenda! {line nil})
  (let [from (or (get @view :from) (today))
        made (agenda from (agenda-days))]
    (swap! view assoc :from from :rows made.rows)
    (open-buffer agenda-buffer (vec (map (fn [given] (update given :line (fn [parts] (vec (filter some? parts))))) made.lines))
                 :keys {"RET" 'calendar-show "a" 'calendar-add "e" 'calendar-edit "d" 'calendar-delete
                        "f" 'calendar-forward "b" 'calendar-backward "t" 'calendar-today "n" 'calendar-next
                        "p" 'calendar-previous "m" 'calendar-month "w" 'calendar-week "i" 'calendar-import
                        "s" 'calendar-sync "g" 'calendar-refresh "q" 'quit-window}
                 :line (or line (first (keep (fn [[index row]] (when row index)) (zip (range (count made.rows)) made.rows))) first-row))))

(def (redraw!)
  (when (buffer-exists? agenda-buffer)
    (let [made (agenda (or (get @view :from) (today)) (agenda-days))]
      (swap! view assoc :rows made.rows)
      (set-lines agenda-buffer (vec (map (fn [given] (update given :line (fn [parts] (vec (filter some? parts))))) made.lines)))))
  (when (buffer-exists? month-buffer) (draw-month!))
  (when (buffer-exists? week-buffer) (draw-week!)))

(def (occurrence-here)
  (cond ((= (buffer-name) agenda-buffer) (get (get @view :rows) (current-line)))
    ((= (buffer-name) event-buffer) (get @view :shown))
    (else nil)))

(defcommand calendar "Show the calendar's agenda"
  (quietly (fn []
             (store/load!)
             (show-agenda!)
             (reminders/watch!))))

(defcommand calendar-refresh "Read the calendars again"
  (quietly (fn []
             (store/load!)
             (redraw!))))

(def (move-days! days)
  (swap! view update :from (fn [from] (+ (or from (today)) days)))
  (show-agenda!))

(defcommand calendar-forward "Show the next week" (move-days! 7))
(defcommand calendar-backward "Show the previous week" (move-days! -7))
(defcommand calendar-today "Show the agenda from today"
  (swap! view assoc :from (today))
  (show-agenda!))

(def (step-row! delta)
  (let [rows (get @view :rows)
        line (current-line)
        targets (vec (keep (fn [[index row]] (when row index)) (zip (range (count rows)) rows)))
        next-one (if (> delta 0)
                   (find (fn [index] (> index line)) targets)
                   (last (filter (fn [index] (< index line)) targets)))]
    (when next-one
      (goto-line next-one))))

(defcommand calendar-next "Move to the next event" (step-row! 1))
(defcommand calendar-previous "Move to the previous event" (step-row! -1))

(def (attendee-line person)
  {line: [[(string/pad-end "" 11) :comment] (str (or person.name person.email) " ") [(string/lower (or person.status "")) :comment]]})

(def (field label value)
  {line: [[(string/pad-end label 11) :comment] value]})

(def (when-text occurrence)
  (if occurrence.all-day
    (let [last-day (dec (day-of occurrence.end))]
      (if (> last-day (day-of occurrence.start))
        (str (time/format occurrence.start "%A %-d %B") " – " (time/format (day-start last-day) "%A %-d %B %Y"))
        (time/format occurrence.start "%A %-d %B %Y, all day")))
    (str (time/format occurrence.start "%A %-d %B %Y, %H:%M") "–" (clock occurrence.end))))

(def (repeat-text rule)
  (let [named (some (fn [[name value]] (when (= value rule) name)) (entries repeats))]
    (or named rule)))

(def (event-lines occurrence)
  (vec (filter some?
               (concat [{line: [[occurrence.summary :heading]]}
                        {line: [["e edits  d deletes  a accepts  x declines  ? maybe  q quits" :comment]]}
                        {line: []}
                        (field "When" (when-text occurrence))
                        (when occurrence.location (field "Where" occurrence.location))
                        (field "Calendar" [occurrence.calendar (calendar-face occurrence.calendar)])
                        (when occurrence.rrule (field "Repeats" (repeat-text occurrence.rrule)))
                        (field "Reminder" (let [alarm (first (or occurrence.alarms []))]
                                            (cond ((nil? alarm) (str (reminders/default-minutes) " minutes before, by default"))
                                              (alarm.before (str (// alarm.before 60000) " minutes before"))
                                              (else (time/format alarm.at "%-d %B %H:%M")))))
                        (when occurrence.organizer (field "Organizer" (or occurrence.organizer.name occurrence.organizer.email)))
                        (when (not (empty? (or occurrence.attendees []))) (field "Guests" ""))]
                       (map attendee-line (or occurrence.attendees []))
                       (if occurrence.description
                         (concat [{line: []}] (map (fn [line] {line: line}) (string/lines occurrence.description)))
                         [])))))

(def (show-event! occurrence)
  (swap! view assoc :shown occurrence)
  (open-buffer event-buffer (event-lines occurrence)
               :keys {"e" 'calendar-edit "d" 'calendar-delete "a" 'calendar-accept "x" 'calendar-decline
                      "?" 'calendar-maybe "q" 'quit-window}))

(defcommand calendar-show "Show the event on this line"
  (if-let [occurrence (occurrence-here)]
    (show-event! occurrence)
    (message "No event on this line")))

(def (default-reminder)
  (reminders/default-minutes))

(def (parse-date text)
  (let [match (regex/find #"^(\d{4})-(\d{1,2})-(\d{1,2})$" (string/trim text))]
    (when match
      (let [[year month day] (map string/to-number match.groups)]
        (time/day-number {year: year month: month day: day})))))

(def (parse-clock text)
  (let [match (regex/find #"^(\d{1,2})(?::(\d{2}))?$" (string/trim text))]
    (when match
      {hour: (string/to-number (first match.groups)) minute: (or (string/to-number (or (second match.groups) "0")) 0)})))

(def (ask question {initial ""})
  (await (prompt question :initial initial)))

(def (ask-date question initial)
  (or (parse-date (ask question :initial (time/format (day-start initial) "%Y-%m-%d")))
      (raise (error "Write dates like 2026-10-23"))))

(def (choose-calendar initial)
  (let [names (map :name (store/calendars))]
    (if (= (count names) 1)
      (first names)
      (await (prompt "Calendar: " :choices names :initial (or initial (first names)))))))

(def (timing day initial)
  "Asks when an event starts and ends on day, returning :start, :end and :all-day."
  (let [start-text (ask "Starts at (empty for all day): " :initial (if (and initial (not initial.all-day)) (clock initial.start) ""))]
    (if (string/blank? start-text)
      {start: (day-start day) end: (day-start (inc day)) all-day: true}
      (let [begins (or (parse-clock start-text) (raise (error "Write times like 9:30")))
            start (time/instant (merge (time/date-of day) begins))
            end-text (ask "Ends at: " :initial (clock (if (and initial (not initial.all-day)) (+ start (- initial.end initial.start)) (time/add start {hours: 1}))))
            ends (or (parse-clock end-text) (raise (error "Write times like 10:30")))
            end (time/instant (merge (time/date-of day) ends))]
        {start: start end: (if (<= end start) (time/add end {days: 1}) end) all-day: false}))))

(defcommand calendar-add "Add an event"
  (quietly (fn []
             (let [title (string/trim (ask "Title: "))]
               (when (string/blank? title)
                 (raise (error "An event needs a title")))
               (let [day (ask-date "Date: " (or (get @view :day) (today)))
                     when (timing day nil)
                     repeat (await (prompt "Repeats: " :choices ["never" "daily" "weekdays" "weekly" "monthly" "yearly"]))
                     calendar (choose-calendar nil)
                     place (string/trim (ask "Where (optional): "))
                     event (merge when
                                  {uid: (store/new-uid) summary: title calendar: calendar
                                   zone: (when (not when.all-day) (time/zone))
                                   location: (when (not (string/blank? place)) place)
                                   rrule: (get repeats repeat)
                                   alarms: [{before: (time/minutes (default-reminder))}]})]
                 (store/save-event! event)
                 (redraw!)
                 (message (str "Added " title)))))))

(def (target)
  (or (occurrence-here) (raise (error "Put point on an event first"))))

(defcommand calendar-edit "Change the event on this line"
  (quietly (fn []
             (let [occurrence (target)
                   master occurrence.event
                   what (await (prompt "Change: " :choices ["title" "time" "date" "place" "repeats" "reminder" "notes"]))
                   changed (cond ((= what "title") (assoc master :summary (string/trim (ask "Title: " :initial master.summary))))
                             ((= what "place") (assoc master :location (let [place (string/trim (ask "Where: " :initial (or master.location "")))] (when (not= place "") place))))
                             ((= what "notes") (assoc master :description (ask "Notes: " :initial (or master.description ""))))
                             ((= what "repeats") (assoc master :rrule (get repeats (await (prompt "Repeats: " :choices ["never" "daily" "weekdays" "weekly" "monthly" "yearly"])))))
                             ((= what "reminder") (assoc master :alarms [{before: (time/minutes (or (string/to-number (ask "Minutes before: " :initial (str (default-reminder)))) (default-reminder)))}]))
                             ((= what "date")
                              (let [day (ask-date "Date: " (day-of master.start))
                                    moved (- (day-start day) (day-start (day-of master.start)))]
                                (merge master {start: (+ master.start moved) end: (+ master.end moved)})))
                             (else (merge master (timing (day-of master.start) master))))]
               (store/save-event! changed)
               (redraw!)
               (when (= (buffer-name) event-buffer)
                 (show-event! (merge occurrence changed)))
               (message (str "Changed " changed.summary))))))

(defcommand calendar-delete "Delete the event on this line"
  (quietly (fn []
             (let [occurrence (target)]
               (if (and occurrence.rrule (await (yes-or-no (str "Delete only the " (time/format occurrence.start "%-d %B") " one? "))))
                 (store/skip-occurrence! occurrence.event occurrence.start)
                 (when (await (yes-or-no (str "Delete " occurrence.summary (if occurrence.rrule " and all its repeats" "") "? ")))
                   (store/delete-event! occurrence.event)))
               (when (= (buffer-name) event-buffer)
                 (close-buffer event-buffer))
               (redraw!)
               (message (str "Deleted " occurrence.summary))))))

(def (me)
  (string/lower (or (plugin-setting 'calendar :email) "")))

(def (answer! status)
  (let [occurrence (target)
        master occurrence.event
        mine (find (fn [person] (= (string/lower person.email) (me))) (or master.attendees []))]
    (when (string/blank? (me))
      (raise (error "Set your :email in the calendar plugin's config to answer invitations")))
    (unless mine
      (raise (error "You aren't among this event's guests")))
    (let [answered (assoc master :attendees (vec (map (fn [person] (if (= person mine) (assoc person :status status) person)) master.attendees)))
          reply (path/join (or (path/parent (or master.file "")) ".") (str (store/safe master.uid) "-reply.ics"))]
      (store/save-event! answered)
      (await (file/write reply (ics/write-calendar [(merge answered {attendees: [(assoc mine :status status)]})] :method "REPLY")))
      (redraw!)
      (show-event! (merge occurrence answered))
      (message (str "Answered " (string/lower status) "; send " reply " to " (or master.organizer.email "the organizer"))))))

(defcommand calendar-accept "Accept the invitation shown" (quietly (fn [] (answer! "ACCEPTED"))))
(defcommand calendar-decline "Decline the invitation shown" (quietly (fn [] (answer! "DECLINED"))))
(defcommand calendar-maybe "Answer maybe to the invitation shown" (quietly (fn [] (answer! "TENTATIVE"))))

(defcommand calendar-import "Add the events of an .ics file, such as an invitation"
  (quietly (fn []
             (let [file (await (prompt-path "Import .ics file: "))
                   parsed (ics/parse (await (file/read file)))
                   found (vec (map ics/event-of (ics/components parsed "VEVENT")))
                   calendar (choose-calendar nil)]
               (when (empty? found)
                 (raise (error "That file has no events")))
               (each [event found]
                 (store/save-event! (merge event {calendar: calendar file: nil})))
               (redraw!)
               (let [first-one (first found)]
                 (swap! view assoc :from (day-of first-one.start))
                 (show-agenda!)
                 (message (str "Imported " (count found) (if (= (count found) 1) " event" " events")
                               (if (some (fn [event] (not (empty? (or event.attendees [])))) found) "; RET shows it, then a, x or ? answers" ""))))))))

(defcommand calendar-sync "Sync the calendars that have a CalDAV address"
  (quietly (fn []
             (let [remote (filter :url (store/calendars))]
               (if (empty? remote)
                 (message "No calendar has a CalDAV :url to sync with")
                 (let [problems (vec (mapcat (fn [calendar]
                                               (message (str "Syncing " calendar.name "…"))
                                               (map (fn [problem] (str calendar.name ": " problem)) (caldav/sync! calendar)))
                                             remote))]
                   (store/load!)
                   (redraw!)
                   (message (if (empty? problems) "Calendars synced" (string/join "; " problems)))))))))

(def (month-of days)
  (let [date (time/date-of days)]
    {year: date.year month: date.month}))

(def (draw-month!)
  (let [selected (or (get @view :day) (today))
        shown (month-of selected)
        first-day (time/day-number {year: shown.year month: shown.month day: 1})
        start (- first-day (time/weekday-number first-day))
        width (max 10 (// (- (or (buffer-width month-buffer) 84) 1) 7))
        found (store/occurrences (day-start start) (day-start (+ start 42)))
        on-day (fn [day] (filter (fn [found] (and (< found.start (day-start (inc day))) (> (max found.end (inc found.start)) (day-start day)))) found))
        cell (fn [text face]
               (let [shown (string/pad-end (string/truncate text (dec width)) width)]
                 (if face [shown face] shown)))
        week-lines (fn [week]
                     (let [days (range (+ start (* week 7)) (+ start (* week 7) 7))
                           numbers {line: (vec (map (fn [day]
                                                      (let [date (time/date-of day)
                                                            face (cond ((= day selected) :keyword)
                                                                   ((= day (today)) :success)
                                                                   ((not= date.month shown.month) :comment)
                                                                   (else :heading))]
                                                        (if (= day selected)
                                                          [(string/pad-end (str "[" date.day "]") width) face :pill]
                                                          (cell (str " " date.day) face))))
                                                    days))}
                           entry (fn [row]
                                   {line: (vec (map (fn [day]
                                                      (let [here (vec (on-day day))]
                                                        (cond ((and (= row 2) (> (count here) 3)) (cell (str " +" (- (count here) 2) " more") :comment))
                                                          ((< row (count here))
                                                           (let [found (nth here row)]
                                                             (cell (str " " (if found.all-day "" (str (clock (max found.start (day-start day))) " ")) found.summary)
                                                                   (calendar-face found.calendar))))
                                                          (else (cell "" nil)))))
                                                    days))})]
                       (vec (concat [numbers] (map entry (range 3)) [{line: []}]))))]
    (set-lines month-buffer
               (vec (concat [{line: [[(time/format (day-start first-day) "%B %Y") :heading]]}
                             {line: [["f/b days  n/p weeks  </> months  t today  RET agenda  w week  q quits" :comment]]}
                             {line: (vec (map (fn [name] [(string/pad-end (str " " name) width) :comment]) day-names))}]
                            (mapcat week-lines (range 6)))))))

(def month-keys
  {"f" 'calendar-month-forward "b" 'calendar-month-backward "n" 'calendar-month-down "p" 'calendar-month-up
   "<" 'calendar-month-previous ">" 'calendar-month-next "t" 'calendar-month-today "RET" 'calendar-month-day
   "w" 'calendar-week "a" 'calendar "q" 'quit-window})

(defcommand calendar-month "Show a month of the calendar"
  (quietly (fn []
             (store/load!)
             (unless (get @view :day)
               (swap! view assoc :day (today)))
             (open-buffer month-buffer [] :keys month-keys :on-resize (fn [_] (draw-month!)))
             (draw-month!)
             (reminders/watch!))))

(def (move-day! days)
  (swap! view update :day (fn [day] (+ (or day (today)) days)))
  (draw-month!))

(def (move-month! months)
  (let [date (time/date-of (or (get @view :day) (today)))
        index (+ (* date.year 12) (dec date.month) months)
        year (// index 12)
        month (inc (mod index 12))]
    (swap! view assoc :day (time/day-number {year: year month: month day: (min date.day (time/days-in-month year month))}))
    (draw-month!)))

(defcommand calendar-month-forward "Select the next day" (move-day! 1))
(defcommand calendar-month-backward "Select the previous day" (move-day! -1))
(defcommand calendar-month-down "Select the same day next week" (move-day! 7))
(defcommand calendar-month-up "Select the same day last week" (move-day! -7))
(defcommand calendar-month-next "Show the next month" (move-month! 1))
(defcommand calendar-month-previous "Show the previous month" (move-month! -1))
(defcommand calendar-month-today "Select today"
  (swap! view assoc :day (today))
  (draw-month!))
(defcommand calendar-month-day "Show the agenda from the selected day"
  (swap! view assoc :from (or (get @view :day) (today)))
  (show-agenda!))

(def hour-height 44)
(def gutter 0.07)

(def (week-start)
  (let [day (or (get @view :day) (today))]
    (- day (time/weekday-number day))))

(def (week-shapes)
  (let [start (week-start)
        found (store/occurrences (day-start start) (day-start (+ start 7)))
        timed (remove :all-day found)
        hours (map (fn [found] (get (time/local found.start) :hour)) timed)
        first-hour (min 8 (reduce min 24 hours))
        last-hour (max 19 (inc (reduce max 0 (map (fn [found] (get (time/local found.end) :hour)) timed))))
        column (/ (- 1 gutter) 7)
        top 28
        y-of (fn [millis day]
               (let [local (time/local millis)
                     hour (+ local.hour (/ local.minute 60))]
                 (+ top (* (- (if (> (day-of millis) day) last-hour hour) first-hour) hour-height))))
        now (time/now)
        today-day (today)]
    (vec (concat
          (map (fn [index]
                 (let [day (+ start index)]
                   [:text (+ gutter (* index column) 0.005) 4 (time/format (day-start day) "%a %-d")
                    (if (= day today-day) :keyword :heading)]))
               (range 7))
          (mapcat (fn [hour]
                    (let [y (+ top (* (- hour first-hour) hour-height))]
                      [[:line gutter y 1 y [:comment 0.3] 1]
                       [:text 0 (- y 7) (str (if (< hour 10) "0" "") hour ":00") :comment]]))
                  (range first-hour (inc last-hour)))
          (map (fn [index] [:line (+ gutter (* index column)) top (+ gutter (* index column)) (+ top (* (- last-hour first-hour) hour-height)) [:comment 0.3] 1])
               (range 8))
          (mapcat (fn [found]
                    (mapcat (fn [index]
                              (let [day (+ start index)
                                    begins (max found.start (day-start day))
                                    ends (min found.end (day-start (inc day)))]
                                (if (< begins ends)
                                  (let [y (y-of begins day)
                                        bottom (max (+ y 18) (y-of ends day))
                                        x (+ gutter (* index column) 0.003)
                                        face (calendar-face found.calendar)]
                                    [[:rect x y (- column 0.006) (- bottom y 2) [face 0.3]]
                                     [:rect x y 0.003 (- bottom y 2) face]
                                     [:text (+ x 0.006) (+ y 3) (str (clock begins) " " found.summary) :foreground]])
                                  [])))
                            (range 7)))
                  timed)
          (if (and (>= today-day start) (< today-day (+ start 7)))
            (let [index (- today-day start)
                  y (y-of now today-day)]
              [[:line (+ gutter (* index column)) y (+ gutter (* (inc index) column)) y :error 2]])
            [])))))

(def (week-view)
  (let [start (week-start)
        found (filter :all-day (store/occurrences (day-start start) (day-start (+ start 7))))]
    (ui/column (ui/row (ui/text (str "Week of " (time/format (day-start start) "%-d %B %Y")) :face :heading)
                       (ui/space)
                       (ui/text "f/b weeks  t this week  a agenda  m month  q quits" :face :comment))
               (ui/text (if (empty? found)
                          " "
                          (string/join "   " (map (fn [found] (str (time/format found.start "%a") ": " found.summary)) found)))
                        :face :comment)
               (ui/scrollable (ui/canvas (week-shapes) :height 720 :relative true))
               :spacing 6)))

(def (draw-week!)
  (open-view week-buffer week-view :keys {"f" 'calendar-week-forward "b" 'calendar-week-backward "t" 'calendar-week-today
                                          "a" 'calendar "m" 'calendar-month "q" 'quit-window}))

(defcommand calendar-week "Show a week of the calendar as a grid of hours"
  (quietly (fn []
             (store/load!)
             (draw-week!)
             (reminders/watch!))))

(defcommand calendar-week-forward "Show the next week"
  (swap! view update :day (fn [day] (+ (or day (today)) 7)))
  (draw-week!))
(defcommand calendar-week-backward "Show the previous week"
  (swap! view update :day (fn [day] (- (or day (today)) 7)))
  (draw-week!))
(defcommand calendar-week-today "Show this week"
  (swap! view assoc :day (today))
  (draw-week!))

(def (start-reminders!)
  (when (plugin-setting 'calendar :reminders)
    (when-let [place (store/folder)]
      (when (await (file/exists? place))
        (store/load!)
        (unless (empty? (store/events))
          (reminders/watch!))))))

(spawn (start-reminders!))
