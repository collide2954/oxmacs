(module
  (import (ox http) (ox string) (ox time)
          (weather forecast :as f)
          (weather page :as p)))

(def no-location
  "Set the weather location in ~/.oxmacs/init.ox, such as (use-plugin weather :config {location: \"Berlin\"})")

(def stale-after (time/minutes 30))

(defonce state (atom {place: nil looked-up: nil forecast: nil problem: nil fetching: false attempted: nil}))
(defonce ticking (atom false))

(def (fetch-json url)
  (let [reply (await (http/get url))]
    (if (= reply.status 200)
      reply.body
      (raise (error $"Open-Meteo replied {reply.status}")))))

(def (show-mode-line)
  (let [forecast (get @state :forecast)]
    (set-mode-line :weather (when (and (setting :mode-line) forecast) (f/brief forecast)))))

(def (look-up location)
  (if (= (get @state :looked-up) location)
    (get @state :place)
    (f/parse-place (fetch-json (f/geocode-url location)))))

(def (fetch location)
  (try
    (let [place (look-up location)]
      (if (nil? place)
        (swap! state assoc :problem $"No place called {location}; check the weather location" :fetching false)
        (let [forecast (f/parse-forecast (fetch-json (f/forecast-url place (setting :units))))]
          (swap! state assoc :place place :looked-up location :forecast forecast :fetching false)
          (show-mode-line))))
    (catch error e
      (swap! state assoc :problem $"Couldn't reach Open-Meteo: {e.message}" :fetching false))))

(def (refresh)
  (let [location (setting :location)]
    (if (or (nil? location) (string/blank? location))
      (swap! state assoc :problem no-location :fetching false)
      (do
        (swap! state assoc :problem nil :fetching true :attempted (time/now))
        (spawn (fetch location))))))

(def (stale?)
  (let [attempted (get @state :attempted)]
    (or (nil? attempted) (>= (- (time/now) attempted) stale-after))))

(def (keep-fresh)
  (when (and (setting :mode-line) (not @ticking))
    (reset! ticking true)
    (refresh)
    (every (time/minutes 1) (fn [] (when (and (setting :mode-line) (stale?)) (refresh)) nil))))

(def (view)
  (p/page @state (setting :location)))

(defcommand weather "Show the weather for your location"
  (open-view "*Weather*" view :keys {g: 'weather-refresh q: 'quit-window})
  (refresh))

(defcommand weather-refresh "Fetch the forecast again"
  (refresh))

(keep-fresh)
(on-settings-change :weather
                    (fn []
                      (keep-fresh)
                      (show-mode-line)))
