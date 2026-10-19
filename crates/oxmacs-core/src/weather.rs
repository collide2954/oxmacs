use jiff::civil::{Date, DateTime};
use serde::Deserialize;
use thiserror::Error;

use crate::config::WeatherUnits;

const GEOCODING: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST: &str = "https://api.open-meteo.com/v1/forecast";
const CURRENT: &str = "temperature_2m,apparent_temperature,relative_humidity_2m,is_day,weather_code,wind_speed_10m,wind_direction_10m";
const HOURLY: &str = "temperature_2m,precipitation_probability,weather_code";
const DAILY: &str = "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset";
const COMPASS: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const HEX: &[u8; 16] = b"0123456789ABCDEF";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WeatherError {
    #[error("unexpected weather data: {0}")]
    Data(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub region: Option<String>,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

impl Place {
    pub fn label(&self) -> String {
        [
            Some(&self.name),
            self.region.as_ref(),
            self.country.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Clear,
    MainlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    Drizzle,
    FreezingDrizzle,
    Rain,
    FreezingRain,
    Snow,
    Showers,
    SnowShowers,
    Thunderstorm,
    Unknown,
}

impl Condition {
    pub fn from_code(code: u16) -> Self {
        match code {
            0 => Self::Clear,
            1 => Self::MainlyClear,
            2 => Self::PartlyCloudy,
            3 => Self::Overcast,
            45 | 48 => Self::Fog,
            51 | 53 | 55 => Self::Drizzle,
            56 | 57 => Self::FreezingDrizzle,
            61 | 63 | 65 => Self::Rain,
            66 | 67 => Self::FreezingRain,
            71 | 73 | 75 | 77 => Self::Snow,
            80..=82 => Self::Showers,
            85 | 86 => Self::SnowShowers,
            95 | 96 | 99 => Self::Thunderstorm,
            _ => Self::Unknown,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::MainlyClear => "Mainly clear",
            Self::PartlyCloudy => "Partly cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::Drizzle => "Drizzle",
            Self::FreezingDrizzle => "Freezing drizzle",
            Self::Rain => "Rain",
            Self::FreezingRain => "Freezing rain",
            Self::Snow => "Snow",
            Self::Showers => "Showers",
            Self::SnowShowers => "Snow showers",
            Self::Thunderstorm => "Thunderstorm",
            Self::Unknown => "Unknown",
        }
    }

    pub fn symbol(self, day: bool) -> &'static str {
        match self {
            Self::Clear | Self::MainlyClear if day => "☀",
            Self::Clear | Self::MainlyClear => "☾",
            Self::PartlyCloudy => "⛅",
            Self::Overcast | Self::Unknown => "☁",
            Self::Fog => "≈",
            Self::Drizzle | Self::Rain | Self::Showers => "☂",
            Self::FreezingDrizzle | Self::FreezingRain | Self::Snow | Self::SnowShowers => "❄",
            Self::Thunderstorm => "⚡",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Now {
    pub time: DateTime,
    pub temperature: f64,
    pub feels_like: f64,
    pub humidity: f64,
    pub wind_speed: f64,
    pub wind_direction: f64,
    pub condition: Condition,
    pub day: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hour {
    pub time: DateTime,
    pub temperature: Option<f64>,
    pub rain_chance: Option<f64>,
    pub condition: Condition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub date: Date,
    pub condition: Condition,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub rain_chance: Option<f64>,
    pub sunrise: Option<DateTime>,
    pub sunset: Option<DateTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub now: Now,
    pub temperature_unit: String,
    pub wind_unit: String,
    pub hours: Vec<Hour>,
    pub days: Vec<Day>,
}

impl Forecast {
    pub fn upcoming(&self, count: usize) -> &[Hour] {
        let start = self
            .hours
            .iter()
            .position(|hour| {
                hour.time.date() == self.now.time.date() && hour.time.hour() == self.now.time.hour()
            })
            .unwrap_or(0);
        let end = (start + count).min(self.hours.len());
        &self.hours[start..end]
    }
}

#[derive(Deserialize)]
struct Places {
    #[serde(default)]
    results: Vec<RawPlace>,
}

#[derive(Deserialize)]
struct RawPlace {
    name: String,
    latitude: f64,
    longitude: f64,
    admin1: Option<String>,
    country: Option<String>,
}

#[derive(Deserialize)]
struct RawForecast {
    current_units: Units,
    current: RawNow,
    hourly: RawHours,
    daily: RawDays,
}

#[derive(Deserialize)]
struct Units {
    temperature_2m: String,
    wind_speed_10m: String,
}

#[derive(Deserialize)]
struct RawNow {
    time: String,
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: f64,
    is_day: u8,
    weather_code: u16,
    wind_speed_10m: f64,
    wind_direction_10m: f64,
}

#[derive(Deserialize)]
struct RawHours {
    time: Vec<String>,
    temperature_2m: Vec<Option<f64>>,
    precipitation_probability: Vec<Option<f64>>,
    weather_code: Vec<Option<u16>>,
}

#[derive(Deserialize)]
struct RawDays {
    time: Vec<String>,
    weather_code: Vec<Option<u16>>,
    temperature_2m_max: Vec<Option<f64>>,
    temperature_2m_min: Vec<Option<f64>>,
    precipitation_probability_max: Vec<Option<f64>>,
    sunrise: Vec<Option<String>>,
    sunset: Vec<Option<String>>,
}

fn encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0xF)]));
        }
    }
    encoded
}

pub fn geocode_url(name: &str) -> String {
    format!(
        "{GEOCODING}?name={}&count=1&language=en&format=json",
        encode(name.trim())
    )
}

pub fn parse_place(json: &str) -> Result<Option<Place>, WeatherError> {
    let places: Places =
        serde_json::from_str(json).map_err(|error| WeatherError::Data(error.to_string()))?;
    Ok(places.results.into_iter().next().map(|place| Place {
        name: place.name,
        region: place.admin1,
        country: place.country,
        latitude: place.latitude,
        longitude: place.longitude,
    }))
}

pub fn forecast_url(place: &Place, units: WeatherUnits) -> String {
    let imperial = match units {
        WeatherUnits::Metric => "",
        WeatherUnits::Imperial => "&temperature_unit=fahrenheit&wind_speed_unit=mph",
    };
    format!(
        "{FORECAST}?latitude={}&longitude={}&current={CURRENT}&hourly={HOURLY}&daily={DAILY}&timezone=auto&forecast_days=7{imperial}",
        place.latitude, place.longitude
    )
}

fn time(text: &str) -> Result<DateTime, WeatherError> {
    text.parse()
        .map_err(|_| WeatherError::Data(format!("bad time `{text}`")))
}

fn condition(code: Option<&Option<u16>>) -> Condition {
    code.copied()
        .flatten()
        .map_or(Condition::Unknown, Condition::from_code)
}

fn at<T: Copy>(values: &[Option<T>], index: usize) -> Option<T> {
    values.get(index).copied().flatten()
}

pub fn parse_forecast(json: &str) -> Result<Forecast, WeatherError> {
    let raw: RawForecast =
        serde_json::from_str(json).map_err(|error| WeatherError::Data(error.to_string()))?;
    let now = Now {
        time: time(&raw.current.time)?,
        temperature: raw.current.temperature_2m,
        feels_like: raw.current.apparent_temperature,
        humidity: raw.current.relative_humidity_2m,
        wind_speed: raw.current.wind_speed_10m,
        wind_direction: raw.current.wind_direction_10m,
        condition: Condition::from_code(raw.current.weather_code),
        day: raw.current.is_day == 1,
    };
    let hourly = &raw.hourly;
    let hours = hourly
        .time
        .iter()
        .enumerate()
        .map(|(index, text)| {
            Ok(Hour {
                time: time(text)?,
                temperature: at(&hourly.temperature_2m, index),
                rain_chance: at(&hourly.precipitation_probability, index),
                condition: condition(hourly.weather_code.get(index)),
            })
        })
        .collect::<Result<_, WeatherError>>()?;
    let daily = &raw.daily;
    let moment = |values: &[Option<String>], index: usize| {
        values
            .get(index)
            .cloned()
            .flatten()
            .and_then(|text| time(&text).ok())
    };
    let days = daily
        .time
        .iter()
        .enumerate()
        .map(|(index, text)| {
            Ok(Day {
                date: text
                    .parse()
                    .map_err(|_| WeatherError::Data(format!("bad date `{text}`")))?,
                condition: condition(daily.weather_code.get(index)),
                high: at(&daily.temperature_2m_max, index),
                low: at(&daily.temperature_2m_min, index),
                rain_chance: at(&daily.precipitation_probability_max, index),
                sunrise: moment(&daily.sunrise, index),
                sunset: moment(&daily.sunset, index),
            })
        })
        .collect::<Result<_, WeatherError>>()?;
    Ok(Forecast {
        now,
        temperature_unit: raw.current_units.temperature_2m,
        wind_unit: raw.current_units.wind_speed_10m,
        hours,
        days,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a compass bearing rounds to one of sixteen points"
)]
pub fn compass(degrees: f64) -> &'static str {
    let point = (degrees.rem_euclid(360.0) / 22.5).round() as usize % COMPASS.len();
    COMPASS[point]
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "each value scales into one of eight bars"
)]
pub fn sparkline(values: &[Option<f64>]) -> String {
    let known = values.iter().flatten();
    let low = known.clone().copied().fold(f64::INFINITY, f64::min);
    let high = known.copied().fold(f64::NEG_INFINITY, f64::max);
    let top = (BARS.len() - 1) as f64;
    values
        .iter()
        .map(|value| match value {
            None => ' ',
            Some(_) if (high - low).abs() < f64::EPSILON => BARS[BARS.len() / 2 - 1],
            Some(value) => BARS[((value - low) / (high - low) * top).round() as usize],
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WeatherUnits;

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

    #[test]
    fn places_are_looked_up_by_name() {
        assert_eq!(
            geocode_url("São Paulo"),
            "https://geocoding-api.open-meteo.com/v1/search?name=S%C3%A3o%20Paulo&count=1&language=en&format=json"
        );

        let place = parse_place(PLACES).unwrap().unwrap();

        assert_eq!(place.name, "Berlin");
        assert_eq!(place.label(), "Berlin, Land Berlin, Germany");
        assert!((place.latitude - 52.524_37).abs() < 1e-9);
    }

    #[test]
    fn unknown_places_are_none() {
        assert_eq!(parse_place(r#"{"generationtime_ms":0.5}"#).unwrap(), None);
        assert!(parse_place("not json").is_err());
    }

    #[test]
    fn forecasts_ask_for_the_configured_units() {
        let place = parse_place(PLACES).unwrap().unwrap();

        let metric = forecast_url(&place, WeatherUnits::Metric);
        let imperial = forecast_url(&place, WeatherUnits::Imperial);

        assert!(metric.starts_with(
            "https://api.open-meteo.com/v1/forecast?latitude=52.52437&longitude=13.41053&"
        ));
        assert!(metric.contains("&timezone=auto&forecast_days=7"));
        assert!(!metric.contains("fahrenheit"));
        assert!(imperial.ends_with("&temperature_unit=fahrenheit&wind_speed_unit=mph"));
    }

    #[test]
    fn forecasts_are_read_from_open_meteo_json() {
        let forecast = parse_forecast(FORECAST).unwrap();

        assert_eq!(forecast.now.condition, Condition::Overcast);
        assert_eq!(forecast.now.temperature.round(), 13.0);
        assert_eq!(forecast.temperature_unit, "°C");
        assert_eq!(forecast.wind_unit, "km/h");
        assert_eq!(forecast.days.len(), 2);
        assert_eq!(forecast.days[1].condition, Condition::Thunderstorm);
        assert_eq!(forecast.days[1].rain_chance, Some(85.0));
        assert_eq!(forecast.hours[3].temperature, None);
        assert_eq!(forecast.hours[2].rain_chance, None);
    }

    #[test]
    fn upcoming_hours_start_at_the_current_hour() {
        let forecast = parse_forecast(FORECAST).unwrap();

        let hours = forecast.upcoming(2);

        assert_eq!(hours.len(), 2);
        assert_eq!(hours[0].time.hour(), 14);
        assert_eq!(forecast.upcoming(10).len(), 3);
    }

    #[test]
    fn weather_codes_follow_the_wmo_table() {
        let cases = [
            (0, Condition::Clear),
            (1, Condition::MainlyClear),
            (2, Condition::PartlyCloudy),
            (3, Condition::Overcast),
            (45, Condition::Fog),
            (53, Condition::Drizzle),
            (57, Condition::FreezingDrizzle),
            (63, Condition::Rain),
            (66, Condition::FreezingRain),
            (75, Condition::Snow),
            (77, Condition::Snow),
            (81, Condition::Showers),
            (86, Condition::SnowShowers),
            (99, Condition::Thunderstorm),
            (42, Condition::Unknown),
        ];
        for (code, condition) in cases {
            assert_eq!(Condition::from_code(code), condition, "{code}");
        }
        assert_eq!(Condition::PartlyCloudy.describe(), "Partly cloudy");
    }

    #[test]
    fn winds_are_named_by_where_they_come_from() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(250.0), "WSW");
        assert_eq!(compass(350.0), "N");
    }

    #[test]
    fn sparklines_scale_between_the_extremes() {
        assert_eq!(sparkline(&[Some(0.0), Some(7.0), None, Some(3.5)]), "▁█ ▅");
        assert_eq!(sparkline(&[Some(2.0), Some(2.0)]), "▄▄");
    }
}
