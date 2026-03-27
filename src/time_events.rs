//! Time-based events for night mode and sunrise/sunset.

use chrono::{Datelike, Local, NaiveTime, Timelike};
use tracing::{debug, info};

use crate::config::{LocationConfig, NightModeConfig};

/// Time event types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeEvent {
    /// Entering night mode
    NightStart,
    /// Exiting night mode
    NightEnd,
    /// Sunrise occurred
    Sunrise,
    /// Sunset occurred
    Sunset,
}

/// Calculate sunrise and sunset times for a given date and location.
pub struct SunTimes {
    pub sunrise: NaiveTime,
    pub sunset: NaiveTime,
}

impl SunTimes {
    /// Calculate sun times for today at the given location.
    pub fn for_today(latitude: f64, longitude: f64) -> Option<Self> {
        let now = Local::now();
        let day_of_year = now.ordinal() as i64;

        // Calculate approximate sun times
        // Note: astro crate works with equatorial coordinates
        // For simplicity, we'll use a basic sunrise/sunset formula

        let sunrise_hour = calculate_sun_time(latitude, day_of_year, true);
        let sunset_hour = calculate_sun_time(latitude, day_of_year, false);

        // Adjust for longitude (rough approximation)
        // 15 degrees = 1 hour
        let longitude_offset = longitude / 15.0;

        let sunrise_adjusted = sunrise_hour - longitude_offset;
        let sunset_adjusted = sunset_hour - longitude_offset;

        // Convert to local time (very approximate)
        let sunrise_minutes = ((sunrise_adjusted.fract().abs()) * 60.0) as u32;
        let sunset_minutes = ((sunset_adjusted.fract().abs()) * 60.0) as u32;

        let sunrise =
            NaiveTime::from_hms_opt(sunrise_adjusted.abs() as u32 % 24, sunrise_minutes, 0)?;
        let sunset =
            NaiveTime::from_hms_opt(sunset_adjusted.abs() as u32 % 24, sunset_minutes, 0)?;

        Some(Self { sunrise, sunset })
    }
}

/// Calculate approximate sunrise or sunset hour using a simplified formula.
/// Returns hours in UTC.
fn calculate_sun_time(latitude: f64, day_of_year: i64, is_sunrise: bool) -> f64 {
    // Simplified sunrise/sunset calculation
    // Based on NOAA Solar Calculator equations

    let lat_rad = latitude.to_radians();

    // Fractional year (gamma)
    let gamma = 2.0 * std::f64::consts::PI * (day_of_year as f64 - 1.0) / 365.0;

    // Equation of time (minutes)
    let eq_time = 229.18 * (0.000075 + 0.001868 * gamma.cos() - 0.032077 * gamma.sin()
        - 0.014615 * (2.0 * gamma).cos()
        - 0.040849 * (2.0 * gamma).sin());

    // Solar declination (radians)
    let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();

    // Hour angle
    let zenith = 90.833_f64.to_radians(); // Official sunrise/sunset
    let cos_hour_angle = (zenith.cos() / (lat_rad.cos() * decl.cos())) - lat_rad.tan() * decl.tan();

    // Clamp to valid range
    let cos_ha = cos_hour_angle.clamp(-1.0, 1.0);
    let hour_angle = cos_ha.acos().to_degrees();

    // Calculate time
    if is_sunrise {
        (720.0 - 4.0 * hour_angle - eq_time) / 60.0
    } else {
        (720.0 + 4.0 * hour_angle - eq_time) / 60.0
    }
}

/// Night mode manager.
pub struct NightModeManager {
    config: NightModeConfig,
    location: Option<LocationConfig>,
    cached_sun_times: Option<(chrono::NaiveDate, SunTimes)>,
}

impl NightModeManager {
    /// Create a new night mode manager.
    pub fn new(config: &NightModeConfig, location: Option<&LocationConfig>) -> Self {
        Self {
            config: config.clone(),
            location: location.cloned(),
            cached_sun_times: None,
        }
    }

    /// Check if we're currently in night mode.
    pub fn is_night_mode(&mut self) -> bool {
        if !self.config.enabled {
            return false;
        }

        let now = Local::now();
        let current_hour = now.hour() as u8;

        if self.config.use_sun_times {
            self.is_night_from_sun_times()
        } else {
            self.is_night_from_hours(current_hour)
        }
    }

    /// Check night mode based on fixed hours.
    fn is_night_from_hours(&self, current_hour: u8) -> bool {
        let start = self.config.start_hour;
        let end = self.config.end_hour;

        if start > end {
            // Night spans midnight (e.g., 22:00 to 06:00)
            current_hour >= start || current_hour < end
        } else {
            // Night is within same day
            current_hour >= start && current_hour < end
        }
    }

    /// Check night mode based on sunrise/sunset.
    fn is_night_from_sun_times(&mut self) -> bool {
        let location = match &self.location {
            Some(loc) if loc.latitude.is_some() && loc.longitude.is_some() => loc,
            _ => {
                debug!("No location configured, falling back to fixed hours");
                let now = Local::now();
                return self.is_night_from_hours(now.hour() as u8);
            }
        };

        let today = Local::now().date_naive();

        // Use cached sun times if available for today
        let sun_times = match &self.cached_sun_times {
            Some((date, times)) if *date == today => times,
            _ => {
                // Calculate new sun times
                if let Some(times) =
                    SunTimes::for_today(location.latitude.unwrap(), location.longitude.unwrap())
                {
                    info!("Calculated sun times: sunrise {:?}, sunset {:?}", times.sunrise, times.sunset);
                    self.cached_sun_times = Some((today, times));
                    &self.cached_sun_times.as_ref().unwrap().1
                } else {
                    debug!("Failed to calculate sun times, falling back to fixed hours");
                    let now = Local::now();
                    return self.is_night_from_hours(now.hour() as u8);
                }
            }
        };

        let now = Local::now().time();

        // Apply sunset delay
        let sunset_with_delay = sun_times.sunset
            + chrono::Duration::seconds(self.config.sundown_delay_secs as i64);

        // Night is after sunset+delay or before sunrise
        now >= sunset_with_delay || now < sun_times.sunrise
    }

    /// Get the next time event (for scheduling).
    pub fn next_event(&mut self) -> Option<(TimeEvent, chrono::DateTime<Local>)> {
        if !self.config.enabled {
            return None;
        }

        let now = Local::now();

        if self.config.use_sun_times {
            // Calculate based on sun times
            // TODO: Implement proper scheduling
            None
        } else {
            // Calculate based on fixed hours
            let today = now.date_naive();

            let night_start = today
                .and_hms_opt(self.config.start_hour as u32, 0, 0)?
                .and_local_timezone(Local)
                .single()?;

            let night_end = today
                .and_hms_opt(self.config.end_hour as u32, 0, 0)?
                .and_local_timezone(Local)
                .single()?;

            if now < night_start && !self.is_night_mode() {
                Some((TimeEvent::NightStart, night_start))
            } else if now < night_end && self.is_night_mode() {
                Some((TimeEvent::NightEnd, night_end))
            } else {
                // Next event is tomorrow
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_night_from_hours_spanning_midnight() {
        let config = NightModeConfig {
            enabled: true,
            use_sun_times: false,
            start_hour: 22,
            end_hour: 6,
            ..Default::default()
        };

        let manager = NightModeManager::new(&config, None);

        assert!(manager.is_night_from_hours(23)); // 11 PM
        assert!(manager.is_night_from_hours(0)); // Midnight
        assert!(manager.is_night_from_hours(5)); // 5 AM
        assert!(!manager.is_night_from_hours(6)); // 6 AM (end)
        assert!(!manager.is_night_from_hours(12)); // Noon
        assert!(!manager.is_night_from_hours(21)); // 9 PM
        assert!(manager.is_night_from_hours(22)); // 10 PM (start)
    }

    #[test]
    fn test_is_night_from_hours_same_day() {
        let config = NightModeConfig {
            enabled: true,
            use_sun_times: false,
            start_hour: 2,
            end_hour: 5,
            ..Default::default()
        };

        let manager = NightModeManager::new(&config, None);

        assert!(!manager.is_night_from_hours(1)); // 1 AM
        assert!(manager.is_night_from_hours(2)); // 2 AM (start)
        assert!(manager.is_night_from_hours(3)); // 3 AM
        assert!(manager.is_night_from_hours(4)); // 4 AM
        assert!(!manager.is_night_from_hours(5)); // 5 AM (end)
    }

    #[test]
    fn test_sun_calculation() {
        // Test for a known location (New York City area)
        let sunrise = calculate_sun_time(40.7, 172, true); // ~June 21
        let sunset = calculate_sun_time(40.7, 172, false);

        // Sunrise should be early (around 4-6 UTC for summer solstice)
        assert!(sunrise > 0.0 && sunrise < 12.0);
        // Sunset should be late (around 19-21 UTC)
        assert!(sunset > 12.0 && sunset < 24.0);
    }
}
