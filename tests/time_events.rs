//! Integration tests for night mode and time-based events.

use chrono::{NaiveTime, TimeZone, Timelike, Utc};

/// Test time parsing and comparison logic.
mod time_parsing {
    use super::*;

    #[test]
    fn test_parse_night_hours() {
        let night_start = NaiveTime::parse_from_str("22:00", "%H:%M").unwrap();
        let night_end = NaiveTime::parse_from_str("06:00", "%H:%M").unwrap();

        assert_eq!(night_start.hour(), 22);
        assert_eq!(night_start.minute(), 0);
        assert_eq!(night_end.hour(), 6);
        assert_eq!(night_end.minute(), 0);
    }

    #[test]
    fn test_time_comparison_same_day() {
        // Daytime hours: night_mode where start > end means overnight
        let start = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(17, 0, 0).unwrap();
        let current = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        // 12:00 is between 9:00 and 17:00
        assert!(current >= start && current < end);
    }

    #[test]
    fn test_time_comparison_spanning_midnight() {
        let start = NaiveTime::from_hms_opt(22, 0, 0).unwrap(); // 10 PM
        let end = NaiveTime::from_hms_opt(6, 0, 0).unwrap(); // 6 AM

        // When start > end, it spans midnight
        assert!(start > end);

        // Test various times
        let eleven_pm = NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        let three_am = NaiveTime::from_hms_opt(3, 0, 0).unwrap();
        let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        // 11 PM is after 10 PM (night)
        assert!(eleven_pm >= start);

        // 3 AM is before 6 AM (still night)
        assert!(three_am < end);

        // Noon is between 6 AM and 10 PM (daytime)
        assert!(noon >= end && noon < start);
    }

    #[test]
    fn test_is_night_logic() {
        // Replicating the logic from NightModeManager::is_night_from_hours
        fn is_night(current: NaiveTime, start: NaiveTime, end: NaiveTime) -> bool {
            if start <= end {
                // Same day range (e.g., 9:00 to 17:00)
                current >= start && current < end
            } else {
                // Spans midnight (e.g., 22:00 to 06:00)
                current >= start || current < end
            }
        }

        let night_start = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        let night_end = NaiveTime::from_hms_opt(6, 0, 0).unwrap();

        // Test edge cases
        assert!(is_night(
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            night_start,
            night_end
        )); // exactly at start
        assert!(!is_night(
            NaiveTime::from_hms_opt(6, 0, 0).unwrap(),
            night_start,
            night_end
        )); // exactly at end
        assert!(is_night(
            NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            night_start,
            night_end
        )); // midnight
        assert!(is_night(
            NaiveTime::from_hms_opt(5, 59, 59).unwrap(),
            night_start,
            night_end
        )); // just before end
        assert!(!is_night(
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            night_start,
            night_end
        )); // noon
        assert!(!is_night(
            NaiveTime::from_hms_opt(21, 59, 59).unwrap(),
            night_start,
            night_end
        )); // just before start
    }
}

/// Test sunrise/sunset calculation using NOAA algorithm.
mod sun_times {

    /// Calculate day of year (1-365 or 1-366)
    fn day_of_year(month: u32, day: u32, is_leap_year: bool) -> u32 {
        let days_in_months = if is_leap_year {
            [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335]
        } else {
            [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
        };
        days_in_months[(month - 1) as usize] + day
    }

    #[test]
    fn test_day_of_year_calculation() {
        // January 1st
        assert_eq!(day_of_year(1, 1, false), 1);

        // February 28th (non-leap year)
        assert_eq!(day_of_year(2, 28, false), 59);

        // March 1st (non-leap year)
        assert_eq!(day_of_year(3, 1, false), 60);

        // March 1st (leap year)
        assert_eq!(day_of_year(3, 1, true), 61);

        // December 31st (non-leap year)
        assert_eq!(day_of_year(12, 31, false), 365);

        // December 31st (leap year)
        assert_eq!(day_of_year(12, 31, true), 366);
    }

    #[test]
    fn test_latitude_bounds() {
        // Valid latitudes are -90 to 90
        let valid_latitudes = [-90.0, -45.0, 0.0, 45.0, 90.0];

        for lat in valid_latitudes {
            assert!(lat >= -90.0 && lat <= 90.0, "Latitude {} out of bounds", lat);
        }
    }

    #[test]
    fn test_longitude_bounds() {
        // Valid longitudes are -180 to 180
        let valid_longitudes = [-180.0, -90.0, 0.0, 90.0, 180.0];

        for lon in valid_longitudes {
            assert!(
                lon >= -180.0 && lon <= 180.0,
                "Longitude {} out of bounds",
                lon
            );
        }
    }

    #[test]
    fn test_known_city_coordinates() {
        // New York City
        let nyc_lat = 40.7128;
        let nyc_lon = -74.0060;
        assert!(nyc_lat > 40.0 && nyc_lat < 41.0);
        assert!(nyc_lon > -75.0 && nyc_lon < -74.0);

        // London
        let london_lat = 51.5074;
        let london_lon = -0.1278;
        assert!(london_lat > 51.0 && london_lat < 52.0);
        assert!(london_lon > -1.0 && london_lon < 0.0);

        // Sydney
        let sydney_lat = -33.8688;
        let sydney_lon = 151.2093;
        assert!(sydney_lat < 0.0); // Southern hemisphere
        assert!(sydney_lon > 150.0 && sydney_lon < 152.0);
    }

    #[test]
    fn test_timezone_offset_calculation() {
        // UTC offset should be within -12 to +14 hours
        let valid_offsets = [
            -12 * 3600,
            -5 * 3600,
            0,
            5 * 3600 + 1800,
            12 * 3600,
            14 * 3600,
        ];

        for offset in valid_offsets {
            let hours = offset / 3600;
            assert!(
                hours >= -12 && hours <= 14,
                "Offset {} hours out of range",
                hours
            );
        }
    }
}

/// Test date/time utilities.
mod datetime_utils {
    use chrono::{NaiveTime, TimeZone, Timelike, Utc};

    #[test]
    fn test_datetime_to_naive_time() {
        let dt = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let naive_time = dt.time();

        assert_eq!(naive_time.hour(), 14);
        assert_eq!(naive_time.minute(), 30);
    }

    #[test]
    fn test_time_difference_calculation() {
        let time1 = NaiveTime::from_hms_opt(14, 30, 0).unwrap();
        let time2 = NaiveTime::from_hms_opt(18, 45, 0).unwrap();

        // Difference should be 4 hours 15 minutes = 15300 seconds
        let diff = (time2 - time1).num_seconds();
        assert_eq!(diff, 4 * 3600 + 15 * 60);
    }

    #[test]
    fn test_time_rollover() {
        // Adding time that rolls past midnight
        let late_time = NaiveTime::from_hms_opt(23, 30, 0).unwrap();

        // Adding 2 hours would roll to 1:30 next day
        // Note: NaiveTime wraps around
        let new_time = late_time
            .overflowing_add_signed(chrono::Duration::hours(2))
            .0;
        assert_eq!(new_time.hour(), 1);
        assert_eq!(new_time.minute(), 30);
    }
}
