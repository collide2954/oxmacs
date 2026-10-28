(plugin weather
  :version "0.3.4"
  :opt-in true
  :summary "Forecasts from Open-Meteo"
  :commands [weather weather-refresh]
  :settings {location: {type: :text doc: "City for M-x weather and the mode line, such as Berlin"}
             units: {type: [:metric :imperial] default: :metric doc: "Celsius and km/h, or Fahrenheit and mph"}
             mode-line: {type: :toggle default: false doc: "Current weather beside the clock"}}
  :startup-when :mode-line
  :entry "weather.ox")
