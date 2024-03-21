use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use core::any::Any;
use dust_core::rtc::{self, Date, Time};

pub struct Backend {
    time_offset: Duration,
}

impl Backend {
    pub fn new(time_offset_secondss: i64) -> Self {
        Backend {
            time_offset: Duration::try_seconds(time_offset_secondss).unwrap(),
        }
    }

    pub fn time_offset_seconds(&self) -> i64 {
        self.time_offset.num_seconds()
    }

    pub fn set_time_offset_seconds(&mut self, value: i64) {
        self.time_offset = Duration::try_seconds(value).unwrap();
    }
}

impl rtc::Backend for Backend {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn get_time(&mut self) -> Time {
        let date_time = Local::now() + self.time_offset;
        Time {
            hour: date_time.hour() as u8,
            minute: date_time.minute() as u8,
            second: date_time.second() as u8,
        }
    }

    fn get_date_time(&mut self) -> (Date, Time) {
        let date_time = Local::now() + self.time_offset;
        (
            Date {
                years_since_2000: (date_time.year() - 2000) as u8,
                month: date_time.month() as u8,
                day: date_time.day() as u8,
                days_from_sunday: date_time.weekday().num_days_from_sunday() as u8,
            },
            Time {
                hour: date_time.hour() as u8,
                minute: date_time.minute() as u8,
                second: date_time.second() as u8,
            },
        )
    }

    fn set_date_time(&mut self, (date, time): (Date, Time)) {
        let date = match NaiveDate::from_ymd_opt(
            date.years_since_2000 as i32 + 2000,
            date.month as u32,
            date.day as u32,
        ) {
            Some(date) => date,
            None => return,
        };
        let time =
            match NaiveTime::from_hms_opt(time.hour as u32, time.minute as u32, time.second as u32)
            {
                Some(time) => time,
                None => return,
            };
        self.time_offset = NaiveDateTime::new(date, time) - Local::now().naive_local();
    }
}
