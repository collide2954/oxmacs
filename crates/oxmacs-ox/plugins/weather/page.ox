(module
  (import (ox ui :only [column row text space scrollable])
          (ox string)
          (weather forecast :as f))
  (export page))

(def label-width 10)

(def (line label value)
  (row (text (string/pad-end label label-width) :face :comment) (text value)))

(def (now-section forecast)
  (let [now forecast.now
        unit (string/replace forecast.temperature-unit "°" "")
        today (first forecast.days)]
    (column
     (line "Now" "{(f/describe now.condition)}, {(f/degrees now.temperature)}{unit}, feels like {(f/degrees now.feels-like)}{unit}")
     (line "" "Humidity {(f/percent now.humidity)}   Wind {(round now.wind-speed)} {forecast.wind-unit} from {(f/compass now.wind-direction)}")
     (when (and today today.sunrise today.sunset)
       (line "" "Sunrise {(f/clock today.sunrise)}   Sunset {(f/clock today.sunset)}"))
     (line "" "As of {(f/clock now.time)} local time"))))

(def (hour-line hour)
  (line "" "{(f/clock hour.time)}  {(string/pad-start (f/degrees hour.temperature) 4)}  {(string/pad-start (f/percent hour.rain) 4)}  {(f/describe hour.condition)}"))

(def (hours-section forecast)
  (let [hours (f/upcoming forecast 24)]
    (unless (empty? hours)
      (column
       (space 8)
       (text "Next 24 hours" :face :heading)
       (row (text (string/repeat " " label-width))
            (text (f/sparkline (map :temperature hours)) :face :link))
       (map (fn [i] (hour-line (nth hours i))) (range 0 (count hours) 3))))))

(def (day-line day)
  (line "" "{(f/weekday day.date)} {(string/pad-start (str day.date.day) 2)}  {(string/pad-start (f/degrees day.low) 4)} / {(string/pad-end (f/degrees day.high) 4)}  {(string/pad-start (f/percent day.rain) 4)}  {(f/describe day.condition)}"))

(def (days-section forecast)
  (unless (empty? forecast.days)
    (column
     (space 8)
     (text "Next 7 days" :face :heading)
     (map day-line forecast.days))))

(def (place-name state location)
  (if (and state.place (= state.looked-up location))
    (f/place-label state.place)
    (or location "")))

(def (page state location)
  (let [forecast state.forecast]
    (scrollable
     (column :spacing 2
             (row (text "Weather" :face :heading) (text "  {(place-name state location)}"))
             (text "g refreshes   q quits" :face :comment)
             (space 8)
             (when state.problem (text state.problem :face :error))
             (when state.fetching (text "Fetching the forecast…" :face :comment))
             (when forecast [(now-section forecast) (hours-section forecast) (days-section forecast)])
             (space 8)
             (text "Weather data by Open-Meteo.com" :face :comment)))))
