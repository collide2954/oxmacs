use crate::plugins::Bundle;

pub const BUNDLED: &[Bundle] = &[WEATHER];

const WEATHER: Bundle = Bundle {
    name: "weather",
    files: &[
        ("plugin.ox", include_str!("../plugins/weather/plugin.ox")),
        ("weather.ox", include_str!("../plugins/weather/weather.ox")),
        (
            "forecast.ox",
            include_str!("../plugins/weather/forecast.ox"),
        ),
        ("page.ox", include_str!("../plugins/weather/page.ox")),
    ],
};

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use crate::Ox;
    use crate::views::{Node, ViewBuffer};

    const PLACES: &str = r#"{"results":[{"id":2950159,"name":"Berlin","latitude":52.52437,"longitude":13.41053,"country_code":"DE","timezone":"Europe/Berlin","country":"Germany","admin1":"Land Berlin"}],"generationtime_ms":0.9}"#;

    const FORECAST: &str = r#"{
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
    }"#;

    const IMPORTS: &str =
        "(module (import (weather forecast :as f) (weather page :as p) (ox string)))";

    fn literal(text: &str) -> String {
        let mut out = String::from("\"");
        for character in text.chars() {
            match character {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '{' => out.push_str("{{"),
                '}' => out.push_str("}}"),
                '\n' => out.push_str("\\n"),
                other => out.push(other),
            }
        }
        out.push('"');
        out
    }

    fn started() -> (tempfile::TempDir, Editor, Ox) {
        let home = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        (home, editor, ox)
    }

    fn evaluated(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        let source = source
            .replace("PLACES", &literal(PLACES))
            .replace("FORECAST", &literal(FORECAST));
        match ox.eval(editor, &format!("{IMPORTS} {source}")) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    fn lines(node: &Node, out: &mut Vec<String>) {
        match node {
            Node::Column { children, .. } => {
                for child in children {
                    lines(child, out);
                }
            }
            Node::Row { children, .. } => {
                let mut row = Vec::new();
                for child in children {
                    lines(child, &mut row);
                }
                out.push(row.concat());
            }
            Node::Text { text, .. } => out.push(text.clone()),
            Node::Button { label, .. } => out.push(label.clone()),
            Node::Space { .. } => {}
            Node::Scrollable(child) => lines(child, out),
        }
    }

    fn shown(editor: &Editor, name: &str) -> Vec<String> {
        let id = editor.buffer_named(name).unwrap();
        let mut out = Vec::new();
        lines(
            &editor.applet_buffer::<ViewBuffer>(id).unwrap().1.node,
            &mut out,
        );
        out
    }

    fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
        ox.tick(editor);
    }

    #[test]
    fn the_weather_commands_exist_before_the_plugin_loads() {
        let (_home, editor, _) = started();

        assert!(editor.command_named("weather").is_some());
        assert!(editor.command_named("weather-refresh").is_some());
    }

    #[test]
    fn places_are_looked_up_by_name() {
        let (_home, mut editor, mut ox) = started();

        assert_eq!(
            evaluated(&mut editor, &mut ox, r#"(f/geocode-url "São Paulo")"#),
            r#""https://geocoding-api.open-meteo.com/v1/search?name=S%C3%A3o%20Paulo&count=1&language=en&format=json""#
        );
        assert_eq!(
            evaluated(
                &mut editor,
                &mut ox,
                "(f/place-label (f/parse-place PLACES))"
            ),
            r#""Berlin, Land Berlin, Germany""#
        );
        assert_eq!(
            evaluated(&mut editor, &mut ox, r#"(f/parse-place "{{}}")"#),
            "nil"
        );
    }

    #[test]
    fn forecasts_ask_for_the_configured_units() {
        let (_home, mut editor, mut ox) = started();

        let checks = evaluated(
            &mut editor,
            &mut ox,
            "(let [place (f/parse-place PLACES) metric (f/forecast-url place :metric) imperial (f/forecast-url place :imperial)] [(string/starts-with? metric \"https://api.open-meteo.com/v1/forecast?latitude=52.52437&longitude=13.41053&\") (string/includes? metric \"&timezone=auto&forecast_days=7\") (string/includes? metric \"fahrenheit\") (string/ends-with? imperial \"&temperature_unit=fahrenheit&wind_speed_unit=mph\")])",
        );

        assert_eq!(checks, "[true true false true]");
    }

    #[test]
    fn forecasts_are_read_from_open_meteo_json() {
        let (_home, mut editor, mut ox) = started();

        let read = evaluated(
            &mut editor,
            &mut ox,
            "(let [forecast (f/parse-forecast FORECAST)] [forecast.now.condition (round forecast.now.temperature) forecast.temperature-unit forecast.wind-unit (count forecast.days) (get (nth forecast.days 1) :condition) (get (nth forecast.days 1) :rain) (get (nth forecast.hours 3) :temperature) (get (nth forecast.hours 2) :rain) (map (fn [hour] (f/clock hour.time)) (f/upcoming forecast 2))])",
        );

        assert_eq!(
            read,
            r#"[:overcast 13 "°C" "km/h" 2 :thunderstorm 85 nil nil ["14:00" "15:00"]]"#
        );
    }

    #[test]
    fn small_helpers_format_the_weather() {
        let (_home, mut editor, mut ox) = started();

        assert_eq!(
            evaluated(
                &mut editor,
                &mut ox,
                "[(f/compass 250) (f/compass 0) (f/compass 359) (f/sparkline [1 nil 3 2]) (f/brief (f/parse-forecast FORECAST)) (f/percent nil)]"
            ),
            r#"["WSW" "N" "N" "▁ █▅" "☁ 13°" "–"]"#
        );
    }

    #[test]
    fn the_page_shows_now_the_next_hours_and_the_next_days() {
        let (_home, mut editor, mut ox) = started();

        evaluated(
            &mut editor,
            &mut ox,
            r#"(def forecast (f/parse-forecast FORECAST)) (open-view "*page*" (fn [] (p/page {forecast: forecast place: (f/parse-place PLACES) looked-up: "Berlin"} "Berlin")))"#,
        );
        ox.tick(&mut editor);

        assert_eq!(
            shown(&editor, "*page*"),
            [
                "Weather  Berlin, Land Berlin, Germany",
                "g refreshes   q quits",
                "Now       Overcast, 13°C, feels like 11°C",
                "          Humidity 71%   Wind 12 km/h from WSW",
                "          Sunrise 07:32   Sunset 18:09",
                "          As of 14:15 local time",
                "Next 24 hours",
                "          █▁ ",
                "          14:00   13°   10%  Overcast",
                "Next 7 days",
                "          Mon 19    7° / 14°    10%  Overcast",
                "          Tue 20    9° / 18°    85%  Thunderstorm",
                "Weather data by Open-Meteo.com",
            ]
        );
    }

    #[test]
    fn weather_without_a_location_says_where_to_set_it() {
        let (_home, mut editor, mut ox) = started();
        let id = editor.command_named("weather").unwrap();

        editor.execute(id);
        ox.tick(&mut editor);

        assert_eq!(editor.buffer().name(), "*Weather*");
        let page = shown(&editor, "*Weather*");
        assert!(
            page.contains(
                &"Set the weather location in ~/.oxmacs/init.ox, such as (use-plugin weather :config {location: \"Berlin\"})"
                    .to_owned()
            ),
            "{page:?}"
        );
        press(&mut editor, &mut ox, "g");
        assert_eq!(shown(&editor, "*Weather*"), page);
        press(&mut editor, &mut ox, "q");
        assert_ne!(editor.buffer().name(), "*Weather*");
    }
}
