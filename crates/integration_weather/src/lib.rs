//! Open-Meteo weather integration
//!
//! Client for the Open-Meteo Weather API (<https://open-meteo.com>).
//! Provides current weather conditions and forecasts without requiring an API key.

pub mod client;
mod models;

pub use client::{OpenMeteoClient, WeatherClient, WeatherConfig, WeatherError};
pub use models::{
    CurrentWeather, DailyForecast, Forecast, WeatherCondition, WeatherData, WeatherUnits,
};
