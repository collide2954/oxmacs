use jiff::civil::{DateTime, Weekday};
use jiff::{SignedDuration, Timestamp};

use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::{Editor, Effect, FetchId};
use crate::text::Buffer;
use crate::theme::Highlight;
use crate::weather::{
    Forecast, Place, compass, forecast_url, geocode_url, parse_forecast, parse_place, sparkline,
};

const NAME: &str = "*Weather*";
const HELP: &str = "g refreshes   q quits";
const NO_LOCATION: &str = "Set weather-location in M-x customize or config.toml, such as Berlin";
const CREDIT: &str = "Weather data by Open-Meteo.com";
const LABEL: usize = 10;
const HOURS: usize = 24;
const HOUR_STEP: usize = 3;
const STALE: SignedDuration = SignedDuration::from_mins(30);

#[derive(Debug)]
enum Request {
    Place(String),
    Forecast,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct WeatherState {
    place: Option<(String, Place)>,
    forecast: Option<Forecast>,
    attempted: Option<Timestamp>,
    pending: Option<(FetchId, Request)>,
    problem: Option<String>,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct WeatherView {
    styles: Vec<LineStyle>,
}

impl WeatherView {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
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

fn page(state: &WeatherState, location: Option<&str>) -> Page {
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

impl Editor {
    fn weather_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Weather(_)))
    }

    pub(in crate::editor) fn weather(&mut self) {
        if let Some(index) = self.weather_index() {
            self.select_buffer(index);
        } else {
            let mut buffer = Buffer::with_text(NAME, "");
            buffer.set_read_only(true);
            self.add_buffer_with(buffer, Mode::Weather(WeatherView::default()));
        }
        self.refresh_weather();
    }

    pub(in crate::editor) fn refresh_weather(&mut self) {
        let location = self
            .config
            .weather_location
            .clone()
            .filter(|name| !name.trim().is_empty());
        let Some(name) = location else {
            self.weather.pending = None;
            self.weather.problem = Some(NO_LOCATION.to_owned());
            self.render_weather();
            return;
        };
        self.weather.problem = None;
        let known = self
            .weather
            .place
            .as_ref()
            .filter(|(looked_up, _)| *looked_up == name)
            .map(|(_, place)| place.clone());
        match known {
            Some(place) => {
                let url = forecast_url(&place, self.config.weather_units);
                self.fetch_weather(url, Request::Forecast);
            }
            None => self.fetch_weather(geocode_url(&name), Request::Place(name)),
        }
        self.render_weather();
    }

    fn fetch_weather(&mut self, url: String, request: Request) {
        let id = FetchId(self.next_fetch);
        self.next_fetch += 1;
        self.weather.pending = Some((id, request));
        self.weather.attempted = Some(Timestamp::now());
        self.effects.push(Effect::Fetch(id, url));
    }

    pub(in crate::editor) fn weather_fetched(
        &mut self,
        id: FetchId,
        result: Result<String, String>,
    ) {
        let request = match self.weather.pending.take() {
            Some((pending, request)) if pending == id => request,
            other => {
                self.weather.pending = other;
                return;
            }
        };
        match (request, result) {
            (_, Err(error)) => {
                self.weather.problem = Some(format!("Couldn't reach Open-Meteo: {error}"));
            }
            (Request::Place(name), Ok(json)) => match parse_place(&json) {
                Ok(Some(place)) => {
                    let url = forecast_url(&place, self.config.weather_units);
                    self.weather.place = Some((name, place));
                    self.fetch_weather(url, Request::Forecast);
                }
                Ok(None) => {
                    self.weather.problem =
                        Some(format!("No place called {name}; check weather-location"));
                }
                Err(error) => self.weather.problem = Some(error.to_string()),
            },
            (Request::Forecast, Ok(json)) => match parse_forecast(&json) {
                Ok(forecast) => self.weather.forecast = Some(forecast),
                Err(error) => self.weather.problem = Some(error.to_string()),
            },
        }
        self.render_weather();
    }

    pub fn weather_tick(&mut self, now: Timestamp) {
        if !self.config.mode_line_weather || self.weather.pending.is_some() {
            return;
        }
        let stale = self
            .weather
            .attempted
            .is_none_or(|attempted| now.duration_since(attempted) >= STALE);
        if stale {
            self.refresh_weather();
        }
    }

    pub fn weather_brief(&self) -> Option<String> {
        let now = &self.weather.forecast.as_ref()?.now;
        Some(format!(
            "{} {}",
            now.condition.symbol(now.day),
            degrees(now.temperature)
        ))
    }

    fn render_weather(&mut self) {
        let Some(index) = self.weather_index() else {
            return;
        };
        let page = page(&self.weather, self.config.weather_location.as_deref());
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Weather(view)) = self.buffers.mode_mut(index) {
            view.styles = page.styles;
        }
    }
}
#[cfg(test)]
mod tests {
    use jiff::{SignedDuration, Timestamp};

    use crate::config::{Config, WeatherUnits};
    use crate::editor::test_support::{lines, press, type_text};
    use crate::editor::{Editor, Effect, FetchId};
    use crate::weather::fixtures::{FORECAST, PLACES};
    use crate::weather::{forecast_url, geocode_url, parse_place};

    fn configured(text: &str) -> Editor {
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(text).unwrap());
        editor
    }

    fn weather(editor: &mut Editor) {
        press(editor, "M-x");
        type_text(editor, "weather");
        press(editor, "RET");
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
        forecast_url(&parse_place(PLACES).unwrap().unwrap(), WeatherUnits::Metric)
    }

    fn arrived(editor: &mut Editor) {
        let place = fetches(editor);
        editor.fetched(place[0].0, Ok(PLACES.to_owned()));
        let forecast = fetches(editor);
        editor.fetched(forecast[0].0, Ok(FORECAST.to_owned()));
    }

    #[test]
    fn without_a_location_the_buffer_says_how_to_set_one() {
        let mut editor = Editor::new();

        weather(&mut editor);

        assert_eq!(editor.buffer().name(), "*Weather*");
        assert!(editor.buffer().is_read_only());
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("Set weather-location"))
        );
        assert_eq!(fetches(&mut editor), Vec::new());
    }

    #[test]
    fn the_place_is_looked_up_before_its_forecast() {
        let mut editor = configured("weather-location = \"Berlin\"");
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
    }

    #[test]
    fn refreshing_reuses_the_place() {
        let mut editor = configured("weather-location = \"Berlin\"");
        weather(&mut editor);
        arrived(&mut editor);

        press(&mut editor, "g");

        let again = fetches(&mut editor);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].1, berlin());
    }

    #[test]
    fn a_new_location_is_looked_up_again() {
        let mut editor = configured("weather-location = \"Berlin\"");
        weather(&mut editor);
        arrived(&mut editor);
        editor.apply_config(&Config::parse("weather-location = \"Oslo\"").unwrap());

        press(&mut editor, "g");

        assert_eq!(fetches(&mut editor)[0].1, geocode_url("Oslo"));
    }

    #[test]
    fn failures_are_shown_in_the_buffer() {
        let mut editor = configured("weather-location = \"Nowhere\"");
        weather(&mut editor);
        let place = fetches(&mut editor);

        editor.fetched(place[0].0, Ok(r#"{"generationtime_ms":0.5}"#.to_owned()));
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("No place called Nowhere"))
        );

        press(&mut editor, "g");
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
        let mut editor = configured("weather-location = \"Berlin\"");
        weather(&mut editor);
        let first = fetches(&mut editor);
        press(&mut editor, "g");
        fetches(&mut editor);

        editor.fetched(first[0].0, Ok(PLACES.to_owned()));

        assert_eq!(fetches(&mut editor), Vec::new());
    }

    #[test]
    fn the_mode_line_brief_shows_the_sky_and_temperature() {
        let mut editor = configured("weather-location = \"Berlin\"");
        assert_eq!(editor.weather_brief(), None);
        weather(&mut editor);
        arrived(&mut editor);

        assert_eq!(editor.weather_brief().as_deref(), Some("☁ 13°"));
    }

    #[test]
    fn ticks_fetch_for_the_mode_line_when_the_forecast_is_old() {
        let mut editor = configured("weather-location = \"Berlin\"");
        editor.weather_tick(Timestamp::now());
        assert_eq!(fetches(&mut editor), Vec::new());

        let mut editor = configured("weather-location = \"Berlin\"\nmode-line-weather = true");
        editor.weather_tick(Timestamp::now());
        arrived(&mut editor);

        editor.weather_tick(Timestamp::now() + SignedDuration::from_mins(10));
        assert_eq!(fetches(&mut editor), Vec::new());

        editor.weather_tick(Timestamp::now() + SignedDuration::from_mins(31));
        assert_eq!(fetches(&mut editor)[0].1, berlin());
    }
}
