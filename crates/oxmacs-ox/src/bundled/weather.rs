use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{press, settle};
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

const IMPORTS: &str = "(module (import (weather forecast :as f) (weather page :as p) (ox string)))";

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
    std::fs::write(home.path().join("init.ox"), "(use-plugin weather)").unwrap();
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
        Node::Space { .. } | Node::Canvas { .. } | Node::Image { .. } | Node::Pages(_) => {}
        Node::Scrollable(child) => lines(child, out),
    }
}

fn shown(editor: &Editor, name: &str) -> Vec<String> {
    let id = editor.buffer_named(name).unwrap();
    let mut out = Vec::new();
    lines(
        &editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node,
        &mut out,
    );
    out
}

#[test]
fn the_weather_commands_exist_before_the_plugin_loads() {
    let (_home, editor, _) = started();

    assert!(editor.command_named("weather").is_some());
    assert!(editor.command_named("weather-refresh").is_some());
}

#[test]
fn the_page_shows_now_the_next_hours_and_the_next_days() {
    let (_home, mut editor, mut ox) = started();

    evaluated(
        &mut editor,
        &mut ox,
        r#"(def forecast (f/parse-forecast FORECAST)) (open-view "*page*" (fn [] (p/page {forecast: forecast place: (f/parse-place PLACES) looked-up: "Berlin"} "Berlin")))"#,
    );
    settle(&mut editor, &mut ox);

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
    settle(&mut editor, &mut ox);

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
