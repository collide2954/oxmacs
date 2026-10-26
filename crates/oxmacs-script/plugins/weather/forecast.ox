(module
  (import (ox http) (ox json) (ox string) (ox time))
  (export geocode-url forecast-url parse-place parse-forecast place-label upcoming
          describe icon brief compass sparkline degrees percent clock weekday))

(def geocoding "https://geocoding-api.open-meteo.com/v1/search")
(def endpoint "https://api.open-meteo.com/v1/forecast")
(def current-fields "temperature_2m,apparent_temperature,relative_humidity_2m,is_day,weather_code,wind_speed_10m,wind_direction_10m")
(def hourly-fields "temperature_2m,precipitation_probability,weather_code")
(def daily-fields "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset")

(def (geocode-url name)
  "{geocoding}?name={(http/encode (string/trim name))}&count=1&language=en&format=json")

(def (forecast-url place units)
  (let [imperial (if (= units :imperial) "&temperature_unit=fahrenheit&wind_speed_unit=mph" "")]
    "{endpoint}?latitude={place.latitude}&longitude={place.longitude}&current={current-fields}&hourly={hourly-fields}&daily={daily-fields}&timezone=auto&forecast_days=7{imperial}"))

(def (parse-place text)
  (when-let [found (first (get (json/read text) :results []))]
    {name: found.name
     region: found.admin1
     country: found.country
     latitude: found.latitude
     longitude: found.longitude}))

(def (place-label place)
  (string/join ", " (filter some? [place.name place.region place.country])))

(def (condition code)
  (cond
    [(= code 0) :clear]
    [(= code 1) :mainly-clear]
    [(= code 2) :partly-cloudy]
    [(= code 3) :overcast]
    [(contains? #{45 48} code) :fog]
    [(contains? #{51 53 55} code) :drizzle]
    [(contains? #{56 57} code) :freezing-drizzle]
    [(contains? #{61 63 65} code) :rain]
    [(contains? #{66 67} code) :freezing-rain]
    [(contains? #{71 73 75 77} code) :snow]
    [(contains? #{80 81 82} code) :showers]
    [(contains? #{85 86} code) :snow-showers]
    [(contains? #{95 96 99} code) :thunderstorm]
    [else :unknown]))

(def descriptions
  {clear: "Clear" mainly-clear: "Mainly clear" partly-cloudy: "Partly cloudy"
   overcast: "Overcast" fog: "Fog" drizzle: "Drizzle" freezing-drizzle: "Freezing drizzle"
   rain: "Rain" freezing-rain: "Freezing rain" snow: "Snow" showers: "Showers"
   snow-showers: "Snow showers" thunderstorm: "Thunderstorm" unknown: "Unknown"})

(def (describe sky)
  (get descriptions sky "Unknown"))

(def (icon sky day)
  (cond
    [(contains? #{:clear :mainly-clear} sky) (if day "☀" "☾")]
    [(= sky :partly-cloudy) "⛅"]
    [(= sky :fog) "≈"]
    [(contains? #{:drizzle :rain :showers} sky) "☂"]
    [(contains? #{:freezing-drizzle :freezing-rain :snow :snow-showers} sky) "❄"]
    [(= sky :thunderstorm) "⚡"]
    [else "☁"]))

(def (moment text)
  (when text (time/parse text)))

(def (parse-forecast text)
  (let [raw (json/read text)
        now raw.current
        hourly raw.hourly
        daily raw.daily]
    {now: {time: (time/parse now.time)
           temperature: now.temperature_2m
           feels-like: now.apparent_temperature
           humidity: now.relative_humidity_2m
           wind-speed: now.wind_speed_10m
           wind-direction: now.wind_direction_10m
           condition: (condition now.weather_code)
           day: (= now.is_day 1)}
     temperature-unit: raw.current_units.temperature_2m
     wind-unit: raw.current_units.wind_speed_10m
     hours: (map (fn [i]
                   {time: (time/parse (nth hourly.time i))
                    temperature: (nth hourly.temperature_2m i)
                    rain: (nth hourly.precipitation_probability i)
                    condition: (condition (nth hourly.weather_code i))})
                 (range (count hourly.time)))
     days: (map (fn [i]
                  {date: (time/parse (nth daily.time i))
                   condition: (condition (nth daily.weather_code i))
                   high: (nth daily.temperature_2m_max i)
                   low: (nth daily.temperature_2m_min i)
                   rain: (nth daily.precipitation_probability_max i)
                   sunrise: (moment (nth daily.sunrise i))
                   sunset: (moment (nth daily.sunset i))})
                (range (count daily.time)))}))

(def (same-hour a b)
  (and (= a.year b.year) (= a.month b.month) (= a.day b.day) (= a.hour b.hour)))

(def (upcoming forecast wanted)
  (let [hours forecast.hours
        now forecast.now.time
        start (or (first (filter (fn [i] (same-hour (get (nth hours i) :time) now))
                                 (range (count hours))))
                  0)
        end (min (+ start wanted) (count hours))]
    (map (fn [i] (nth hours i)) (range start end))))

(def points ["N" "NNE" "NE" "ENE" "E" "ESE" "SE" "SSE" "S" "SSW" "SW" "WSW" "W" "WNW" "NW" "NNW"])

(def (compass bearing)
  (nth points (mod (round (/ (mod bearing 360.0) 22.5)) 16)))

(def bars "▁▂▃▄▅▆▇█")

(def (sparkline values)
  (let [known (filter some? values)]
    (if (empty? known)
      (string/repeat " " (count values))
      (let [low (apply min known)
            high (apply max known)]
        (string/join
         (map (fn [value]
                (cond
                  [(nil? value) " "]
                  [(< (abs (- high low)) 0.000001) (nth bars 3)]
                  [else (nth bars (round (* (/ (- value low) (- high low)) 7)))]))
              values))))))

(def (degrees value)
  (if (nil? value) "–" "{(round value)}°"))

(def (percent value)
  (if (nil? value) "–" "{(round value)}%"))

(def (two number)
  (string/pad-start (str number) 2 "0"))

(def (clock moment)
  "{(two moment.hour)}:{(two moment.minute)}")

(def (weekday moment)
  (string/capitalize (string/slice (str moment.weekday) 1 4)))

(def (brief forecast)
  (let [now forecast.now]
    "{(icon now.condition now.day)} {(degrees now.temperature)}"))

(def places-sample #r#"{"results":[{"id":2950159,"name":"Berlin","latitude":52.52437,"longitude":13.41053,"country_code":"DE","timezone":"Europe/Berlin","country":"Germany","admin1":"Land Berlin"}],"generationtime_ms":0.9}"#)

(def forecast-sample #r#"{
    "latitude": 52.52, "longitude": 13.42, "timezone": "Europe/Berlin",
    "current_units": {"temperature_2m": "°C", "wind_speed_10m": "km/h"},
    "current": {"time": "2026-10-19T14:15", "temperature_2m": 13.2, "apparent_temperature": 11.04,
                "relative_humidity_2m": 71, "is_day": 1, "weather_code": 3,
                "wind_speed_10m": 11.5, "wind_direction_10m": 250},
    "hourly": {"time": ["2026-10-19T13:00", "2026-10-19T14:00", "2026-10-19T15:00", "2026-10-19T16:00"],
               "temperature_2m": [12.9, 13.1, 12.4, null],
               "precipitation_probability": [5, 10, null, 40],
               "weather_code": [2, 3, 61, 80]},
    "daily": {"time": ["2026-10-19", "2026-10-20"],
              "weather_code": [3, 95],
              "temperature_2m_max": [14.1, 17.6],
              "temperature_2m_min": [6.5, 9.0],
              "precipitation_probability_max": [10, 85],
              "sunrise": ["2026-10-19T07:32", "2026-10-20T07:34"],
              "sunset": ["2026-10-19T18:09", "2026-10-20T18:07"]}
}"#)

(test "places are looked up by name"
  (expect (= (geocode-url "São Paulo")
             "https://geocoding-api.open-meteo.com/v1/search?name=S%C3%A3o%20Paulo&count=1&language=en&format=json"))
  (expect (= (place-label (parse-place places-sample)) "Berlin, Land Berlin, Germany"))
  (expect (nil? (parse-place "{{}}"))))

(test "forecasts ask for the configured units"
  (let [place (parse-place places-sample)
        metric (forecast-url place :metric)
        imperial (forecast-url place :imperial)]
    (expect (string/starts-with? metric "https://api.open-meteo.com/v1/forecast?latitude=52.52437&longitude=13.41053&"))
    (expect (string/includes? metric "&timezone=auto&forecast_days=7"))
    (expect (not (string/includes? metric "fahrenheit")))
    (expect (string/ends-with? imperial "&temperature_unit=fahrenheit&wind_speed_unit=mph"))))

(test "forecasts are read from Open-Meteo's JSON"
  (let [forecast (parse-forecast forecast-sample)]
    (expect (= forecast.now.condition :overcast))
    (expect (= (round forecast.now.temperature) 13))
    (expect (= [forecast.temperature-unit forecast.wind-unit] ["°C" "km/h"]))
    (expect (= (count forecast.days) 2))
    (expect (= (get (nth forecast.days 1) :condition) :thunderstorm))
    (expect (= (get (nth forecast.days 1) :rain) 85))
    (expect (nil? (get (nth forecast.hours 3) :temperature)))
    (expect (nil? (get (nth forecast.hours 2) :rain)))
    (expect (= (map (fn [hour] (clock hour.time)) (upcoming forecast 2)) ["14:00" "15:00"]))))

(test "small helpers format the weather"
  (expect (= (map compass [250 0 359]) ["WSW" "N" "N"]))
  (expect (= (sparkline [1 nil 3 2]) "▁ █▅"))
  (expect (= (brief (parse-forecast forecast-sample)) "☁ 13°"))
  (expect (= (percent nil) "–")))
