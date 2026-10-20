pub mod forecast;

use iced::Subscription;
use jiff::civil::{DateTime, Weekday};
use jiff::{SignedDuration, Timestamp};
use oxmacs_core::applet::{Applet, AppletBuffer, Payload};
use oxmacs_core::command::{Command, CommandSpec, Scope, ScopeId};
use oxmacs_core::config::schema::{Kind, Setting};
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, Page};
use oxmacs_core::theme::Highlight;
use oxmacs_gui::applet::GuiApplet;
use oxmacs_gui::ticker;
use serde::{Deserialize, Serialize};

use forecast::{
    Forecast, Place, compass, forecast_url, geocode_url, parse_forecast, parse_place, sparkline,
};

const TABLE: &str = "weather";
const NAME: &str = "*Weather*";
const HELP: &str = "g refreshes   q quits";
const NO_LOCATION: &str = "Set the location under Weather in M-x customize, or [weather] location in config.toml, such as Berlin";
const CREDIT: &str = "Weather data by Open-Meteo.com";
const LABEL: usize = 10;
const HOURS: usize = 24;
const HOUR_STEP: usize = 3;
const STALE: SignedDuration = SignedDuration::from_mins(30);
const OPEN_HELP: &str = "Opens *Weather* with the current conditions, the next 24 hours and the next 7 days from Open-Meteo. Set the city with location under Weather in M-x customize, or [weather] location in config.toml, and choose units with units; g refreshes and q quits. mode-line adds the current weather beside the clock.";

const SETTINGS: &[Setting] = &[
    Setting {
        key: "location",
        group: "Weather",
        kind: Kind::Text,
        unset: Some("not set"),
        hint: "City for M-x weather and the mode line, such as Berlin",
    },
    Setting {
        key: "units",
        group: "Weather",
        kind: Kind::Choice(&["metric", "imperial"]),
        unset: None,
        hint: "Celsius and km/h, or Fahrenheit and mph",
    },
    Setting {
        key: "mode-line",
        group: "Weather",
        kind: Kind::Toggle,
        unset: None,
        hint: "Current weather beside the clock",
    },
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub units: Units,
    pub mode_line: bool,
}

#[derive(Debug)]
pub struct Weather;

#[derive(Debug)]
pub struct Tick;

#[derive(Debug)]
enum Request {
    Place(String),
    Forecast,
}

#[derive(Debug, Default)]
struct State {
    scope: Option<ScopeId>,
    place: Option<(String, Place)>,
    forecast: Option<Forecast>,
    attempted: Option<Timestamp>,
    pending: Option<u64>,
    sent: u64,
    problem: Option<String>,
}

#[derive(Debug)]
struct View {
    scope: ScopeId,
    styles: Vec<LineStyle>,
}

impl AppletBuffer for View {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

fn settings(editor: &Editor) -> Settings {
    editor.applet_config::<Settings>(TABLE)
}

fn update(editor: &mut Editor, change: impl FnOnce(&mut State)) {
    if let Some(state) = editor.applet_state_mut::<State>() {
        change(state);
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "temperatures round to whole degrees"
)]
fn degrees(value: f64) -> String {
    format!("{}°", value.round() as i64)
}

fn clock(time: DateTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn chance(value: Option<f64>) -> String {
    value.map_or_else(
        || "–".to_owned(),
        |value| format!("{}%", degrees(value).trim_end_matches('°')),
    )
}

fn row(page: &mut Page, label: &str, text: &str) {
    page.line(&[
        (&format!("{label:<LABEL$}"), Some(Highlight::Comment)),
        (text, None),
    ]);
}

fn now_section(page: &mut Page, forecast: &Forecast) {
    let now = &forecast.now;
    let unit = forecast.temperature_unit.trim_start_matches('°');
    row(
        page,
        "Now",
        &format!(
            "{}, {}{unit}, feels like {}{unit}",
            now.condition.describe(),
            degrees(now.temperature),
            degrees(now.feels_like)
        ),
    );
    row(
        page,
        "",
        &format!(
            "Humidity {}   Wind {} {} from {}",
            chance(Some(now.humidity)),
            degrees(now.wind_speed).trim_end_matches('°'),
            forecast.wind_unit,
            compass(now.wind_direction)
        ),
    );
    if let Some(today) = forecast.days.first()
        && let (Some(sunrise), Some(sunset)) = (today.sunrise, today.sunset)
    {
        row(
            page,
            "",
            &format!("Sunrise {}   Sunset {}", clock(sunrise), clock(sunset)),
        );
    }
    row(page, "", &format!("As of {} local time", clock(now.time)));
}

fn hours_section(page: &mut Page, forecast: &Forecast) {
    let hours = forecast.upcoming(HOURS);
    if hours.is_empty() {
        return;
    }
    page.line(&[]);
    page.heading("Next 24 hours");
    let temperatures: Vec<Option<f64>> = hours.iter().map(|hour| hour.temperature).collect();
    page.line(&[
        (&" ".repeat(LABEL), None),
        (&sparkline(&temperatures), Some(Highlight::Link)),
    ]);
    for hour in hours.iter().step_by(HOUR_STEP) {
        let temperature = hour.temperature.map_or_else(|| "–".to_owned(), degrees);
        row(
            page,
            "",
            &format!(
                "{}  {temperature:>4}  {:>4}  {}",
                clock(hour.time),
                chance(hour.rain_chance),
                hour.condition.describe()
            ),
        );
    }
}

fn days_section(page: &mut Page, forecast: &Forecast) {
    if forecast.days.is_empty() {
        return;
    }
    page.line(&[]);
    page.heading("Next 7 days");
    for day in &forecast.days {
        let low = day.low.map_or_else(|| "–".to_owned(), degrees);
        let high = day.high.map_or_else(|| "–".to_owned(), degrees);
        row(
            page,
            "",
            &format!(
                "{} {:>2}  {low:>4} / {high:<4}  {:>4}  {}",
                weekday(day.date.weekday()),
                day.date.day(),
                chance(day.rain_chance),
                day.condition.describe()
            ),
        );
    }
}

fn page(state: &State, location: Option<&str>) -> Page {
    let mut page = Page::default();
    let place = state
        .place
        .as_ref()
        .filter(|(name, _)| Some(name.as_str()) == location)
        .map(|(_, place)| place.label())
        .or_else(|| location.map(str::to_owned))
        .unwrap_or_default();
    page.line(&[
        ("Weather", Some(Highlight::Heading)),
        (&format!("  {place}"), None),
    ]);
    page.line(&[(HELP, Some(Highlight::Comment))]);
    page.line(&[]);
    if let Some(problem) = &state.problem {
        page.line(&[(problem, Some(Highlight::Error))]);
        page.line(&[]);
    }
    if state.pending.is_some() {
        page.line(&[("Fetching the forecast…", Some(Highlight::Comment))]);
        page.line(&[]);
    }
    if let Some(forecast) = &state.forecast {
        now_section(&mut page, forecast);
        hours_section(&mut page, forecast);
        days_section(&mut page, forecast);
        page.line(&[]);
    }
    page.line(&[(CREDIT, Some(Highlight::Comment))]);
    page
}

fn open(editor: &mut Editor) {
    let Some(scope) = editor.applet_state::<State>().and_then(|state| state.scope) else {
        return;
    };
    match editor.find_applet_buffer::<View>(|_| true) {
        Some(id) => editor.show_buffer(id),
        None => {
            editor.open_applet_buffer(
                NAME,
                View {
                    scope,
                    styles: Vec::new(),
                },
            );
        }
    }
    refresh(editor);
}

fn refresh(editor: &mut Editor) {
    let settings = settings(editor);
    let Some(name) = settings.location.filter(|name| !name.trim().is_empty()) else {
        update(editor, |state| {
            state.pending = None;
            state.problem = Some(NO_LOCATION.to_owned());
        });
        render(editor);
        return;
    };
    let known = editor.applet_state::<State>().and_then(|state| {
        state
            .place
            .as_ref()
            .filter(|(looked_up, _)| *looked_up == name)
            .map(|(_, place)| place.clone())
    });
    update(editor, |state| state.problem = None);
    match known {
        Some(place) => send(
            editor,
            forecast_url(&place, settings.units),
            Request::Forecast,
        ),
        None => send(editor, geocode_url(&name), Request::Place(name)),
    }
    render(editor);
}

fn send(editor: &mut Editor, url: String, request: Request) {
    let mut number = 0;
    update(editor, |state| {
        state.sent += 1;
        number = state.sent;
        state.pending = Some(number);
        state.attempted = Some(Timestamp::now());
    });
    editor.fetch(url, move |editor, result| {
        arrived(editor, number, request, result);
    });
}

fn arrived(editor: &mut Editor, number: u64, request: Request, result: Result<String, String>) {
    if editor
        .applet_state::<State>()
        .and_then(|state| state.pending)
        != Some(number)
    {
        return;
    }
    update(editor, |state| state.pending = None);
    let problem = match (request, result) {
        (_, Err(error)) => Some(format!("Couldn't reach Open-Meteo: {error}")),
        (Request::Place(name), Ok(json)) => match parse_place(&json) {
            Ok(Some(place)) => {
                let url = forecast_url(&place, settings(editor).units);
                update(editor, |state| state.place = Some((name, place)));
                send(editor, url, Request::Forecast);
                None
            }
            Ok(None) => Some(format!(
                "No place called {name}; check the weather location"
            )),
            Err(error) => Some(error.to_string()),
        },
        (Request::Forecast, Ok(json)) => match parse_forecast(&json) {
            Ok(forecast) => {
                update(editor, |state| state.forecast = Some(forecast));
                None
            }
            Err(error) => Some(error.to_string()),
        },
    };
    if problem.is_some() {
        update(editor, |state| state.problem = problem);
    }
    render(editor);
}

fn tick(editor: &mut Editor, now: Timestamp) {
    let Some(state) = editor.applet_state::<State>() else {
        return;
    };
    if !settings(editor).mode_line || state.pending.is_some() {
        return;
    }
    let old = state
        .attempted
        .is_none_or(|attempted| now.duration_since(attempted) >= STALE);
    if old {
        refresh(editor);
    }
}

fn brief(editor: &Editor) -> Option<String> {
    if !settings(editor).mode_line {
        return None;
    }
    let now = &editor.applet_state::<State>()?.forecast.as_ref()?.now;
    Some(format!(
        "{} {}",
        now.condition.symbol(now.day),
        degrees(now.temperature)
    ))
}

fn render(editor: &mut Editor) {
    let Some(id) = editor.find_applet_buffer::<View>(|_| true) else {
        return;
    };
    let location = settings(editor).location;
    let Some(state) = editor.applet_state::<State>() else {
        return;
    };
    let (text, styles) = page(state, location.as_deref()).into_parts();
    if let Some((_, view)) = editor.applet_buffer_mut::<View>(id) {
        view.styles = styles;
    }
    editor.fill_buffer(id, &text);
}

impl Applet for Weather {
    fn name(&self) -> &'static str {
        "weather"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn install(&self, editor: &mut Editor) {
        let scope = editor.add_scope(NAME);
        editor.set_applet_state(State {
            scope: Some(scope),
            ..State::default()
        });
        editor.add_command(CommandSpec {
            name: "weather",
            summary: "Show the weather for your location",
            help: Some(OPEN_HELP),
            scope: Scope::Global.into(),
            run: open,
        });
        let again = editor.add_command(CommandSpec {
            name: "weather-refresh",
            summary: "Fetch the forecast again",
            help: None,
            scope,
            run: refresh,
        });
        editor.bind(scope, "g", again).ok();
        editor.bind(scope, "q", Command::QuitWindow).ok();
        editor.add_config_table::<Settings>(TABLE);
        editor.add_settings(TABLE, SETTINGS);
        editor.add_mode_line_segment(brief);
        editor.on_event(|editor, _: &Tick| tick(editor, Timestamp::now()));
        editor.on_configured(|editor| tick(editor, Timestamp::now()));
        tick(editor, Timestamp::now());
    }
}

impl GuiApplet for Weather {
    fn subscription(&self, editor: &Editor) -> Subscription<Payload> {
        if settings(editor).mode_line {
            ticker::ticks(Some(ticker::MINUTE)).map(|()| Payload::new(Tick))
        } else {
            Subscription::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::{SignedDuration, Timestamp};
    use oxmacs_core::config::Config;
    use oxmacs_core::editor::{Editor, Effect, FetchId};
    use oxmacs_core::input::KeySequence;
    use oxmacs_gui::app::{App, Message};

    use super::forecast::fixtures::{FORECAST, PLACES};
    use super::forecast::{forecast_url, geocode_url, parse_place};
    use super::*;

    fn keys(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    fn typed(editor: &mut Editor, text: &str) {
        let spaced: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        keys(editor, &spaced.join(" "));
    }

    fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn configured(text: &str) -> Editor {
        let mut editor = Editor::new();
        editor.install(&Weather);
        editor.apply_config(&Config::parse(text).unwrap());
        editor
    }

    fn weather(editor: &mut Editor) {
        keys(editor, "M-x");
        typed(editor, "weather");
        keys(editor, "RET");
    }

    fn fetches(editor: &mut Editor) -> Vec<(FetchId, String)> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Fetch(id, url) => Some((id, url)),
                _ => None,
            })
            .collect()
    }

    fn berlin() -> String {
        forecast_url(&parse_place(PLACES).unwrap().unwrap(), Units::Metric)
    }

    fn arrived(editor: &mut Editor) {
        let place = fetches(editor);
        editor.fetched(place[0].0, Ok(PLACES.to_owned()));
        let forecast = fetches(editor);
        editor.fetched(forecast[0].0, Ok(FORECAST.to_owned()));
    }

    #[test]
    fn without_a_location_the_buffer_says_how_to_set_one() {
        let mut editor = configured("");

        weather(&mut editor);

        assert_eq!(editor.buffer().name(), "*Weather*");
        assert!(editor.buffer().is_read_only());
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("Set the location"))
        );
        assert_eq!(fetches(&mut editor), Vec::new());
    }

    #[test]
    fn the_place_is_looked_up_before_its_forecast() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        weather(&mut editor);

        let place = fetches(&mut editor);
        assert_eq!(place.len(), 1);
        assert_eq!(place[0].1, geocode_url("Berlin"));
        assert!(lines(&editor).iter().any(|line| line.contains("Fetching")));

        editor.fetched(place[0].0, Ok(PLACES.to_owned()));
        let forecast = fetches(&mut editor);
        assert_eq!(forecast.len(), 1);
        assert_eq!(forecast[0].1, berlin());

        editor.fetched(forecast[0].0, Ok(FORECAST.to_owned()));
        let shown = lines(&editor);
        assert!(
            shown[0].contains("Berlin, Land Berlin, Germany"),
            "{shown:?}"
        );
        assert!(
            shown
                .iter()
                .any(|line| line.contains("Overcast") && line.contains("13°C")),
            "{shown:?}"
        );
        assert!(
            shown.iter().any(|line| line.contains("Thunderstorm")),
            "{shown:?}"
        );
        assert!(
            shown.iter().any(|line| line.contains("Open-Meteo.com")),
            "{shown:?}"
        );
        assert_eq!(editor.line_style(0).spans[0].1, Highlight::Heading);
    }

    #[test]
    fn the_old_weather_keys_still_work() {
        let mut editor =
            configured("weather-location = \"Berlin\"\nweather-units = \"imperial\"\n");

        assert_eq!(settings(&editor).location.as_deref(), Some("Berlin"));
        assert_eq!(settings(&editor).units, Units::Imperial);
        weather(&mut editor);
        assert_eq!(fetches(&mut editor)[0].1, geocode_url("Berlin"));
    }

    #[test]
    fn g_refreshes_and_reuses_the_place() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        weather(&mut editor);
        arrived(&mut editor);

        keys(&mut editor, "g");

        let again = fetches(&mut editor);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].1, berlin());
    }

    #[test]
    fn a_new_location_is_looked_up_again() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        weather(&mut editor);
        arrived(&mut editor);
        editor.apply_config(&Config::parse("[weather]\nlocation = \"Oslo\"\n").unwrap());

        keys(&mut editor, "g");

        assert_eq!(fetches(&mut editor)[0].1, geocode_url("Oslo"));
    }

    #[test]
    fn failures_are_shown_in_the_buffer() {
        let mut editor = configured("[weather]\nlocation = \"Nowhere\"\n");
        weather(&mut editor);
        let place = fetches(&mut editor);

        editor.fetched(place[0].0, Ok(r#"{"generationtime_ms":0.5}"#.to_owned()));
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("No place called Nowhere"))
        );

        keys(&mut editor, "g");
        let retry = fetches(&mut editor);
        editor.fetched(retry[0].0, Err("timed out".to_owned()));
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("Couldn't reach Open-Meteo: timed out"))
        );
    }

    #[test]
    fn stale_answers_are_ignored() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        weather(&mut editor);
        let first = fetches(&mut editor);
        keys(&mut editor, "g");
        fetches(&mut editor);

        editor.fetched(first[0].0, Ok(PLACES.to_owned()));

        assert_eq!(fetches(&mut editor), Vec::new());
    }

    #[test]
    fn the_mode_line_shows_the_sky_and_temperature_when_asked() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        weather(&mut editor);
        arrived(&mut editor);
        assert_eq!(editor.mode_line_segments(), Vec::<String>::new());

        editor.apply_config(
            &Config::parse("[weather]\nlocation = \"Berlin\"\nmode-line = true\n").unwrap(),
        );

        assert_eq!(editor.mode_line_segments(), ["☁ 13°"]);
    }

    #[test]
    fn turning_the_mode_line_on_fetches_and_ticks_refetch_when_old() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        assert_eq!(fetches(&mut editor), Vec::new());

        editor.apply_config(
            &Config::parse("[weather]\nlocation = \"Berlin\"\nmode-line = true\n").unwrap(),
        );
        arrived(&mut editor);

        tick(
            &mut editor,
            Timestamp::now() + SignedDuration::from_mins(10),
        );
        assert_eq!(fetches(&mut editor), Vec::new());

        tick(
            &mut editor,
            Timestamp::now() + SignedDuration::from_mins(31),
        );
        assert_eq!(fetches(&mut editor)[0].1, berlin());
    }

    #[test]
    fn the_window_mode_line_shows_the_weather() {
        let config = Config::parse("[weather]\nlocation = \"Berlin\"\nmode-line = true\n").unwrap();
        let (mut app, _) = App::boot(&Ok(config), &[], vec![Box::new(Weather)]);

        let _ = app.update(Message::Fetched(FetchId(0), Ok(PLACES.to_owned())));
        let _ = app.update(Message::Fetched(FetchId(1), Ok(FORECAST.to_owned())));

        let mut ui = iced_test::simulator(app.view());
        assert!(ui.find("☁ 13°").is_ok());
    }

    #[test]
    fn the_settings_live_under_their_own_heading() {
        let mut editor = configured("[weather]\nlocation = \"Berlin\"\n");
        keys(&mut editor, "M-x");
        typed(&mut editor, "customize");
        keys(&mut editor, "RET");

        let lines = lines(&editor);
        let heading = lines
            .iter()
            .position(|line| line == "Weather  [weather]")
            .unwrap();
        assert!(
            lines[heading + 1].ends_with(" Berlin"),
            "{}",
            lines[heading + 1]
        );
        assert!(
            lines[heading + 3].ends_with(" [metric]"),
            "{}",
            lines[heading + 3]
        );
        assert!(
            lines[heading + 5].ends_with(" [ ]"),
            "{}",
            lines[heading + 5]
        );
    }
}
