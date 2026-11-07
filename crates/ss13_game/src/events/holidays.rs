//! Holidays (server): tg `/datum/holiday` (`code/modules/holiday/holidays.dm`, `nth_week.dm`,
//! `foreign_calendar.dm`), `fill_holidays` / `check_holidays` (`subsystem/events.dm`) and the
//! round start greeting (`SSticker/setup`).
//!
//! - [`HOLIDAYS`]: every holiday with its dates and timezones: fixed dates, the nth weekday of a
//!   month, Friday the 13th, Programmers' Day, Easter and the Islamic and Hebrew calendar ones.
//!   BandaStation's Pride Week never comes (`modular_bandastation/balance/code/events.dm`), so it
//!   is not listed.
//! - At startup ([`fill`], from `init_events`) the holidays celebrated today in any of their
//!   timezones go into [`Holidays`], shuffled; [`check_holidays`] asks for one by name.
//! - [`greet`]: "и..." and each holiday's greeting after the round start welcome.
//! - Events with a `holidayID` ([`super::EventControl::holiday`]) spawn only on their holiday;
//!   the Chernobyl anniversary doubles the Radiation Storm's weight ([`adjust_controls`]).
// shortcut: no station name prefixes (station names are the map's here), holiday hats, holiday
// mail, posters and decoration colors, and no `celebrate()` side effects (April Fools' clown
// overflow (its lobby music is `crate::lobby_music`), Christmas telescreens and Ian's hat,
// Monkey Day's Pun Pun, holiday maintenance loot); the many item and species checks of single
// holidays live with their mechanics.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};

use super::controller::{EventControl, EventRng};

pub const NEW_YEAR: &str = "New Year";
pub const VALENTINES: &str = "Valentine's Day";
pub const APRIL_FOOLS: &str = "April Fool's Day";
pub const EASTER: &str = "Easter";
pub const HALLOWEEN: &str = "Halloween";
pub const CHRISTMAS: &str = "Christmas";
pub const FESTIVE_SEASON: &str = "Festive Season";
pub const GARBAGEDAY: &str = "Garbage Day";
pub const MONKEYDAY: &str = "Monkey Day";
pub const MOTH_WEEK: &str = "Moth Week";
pub const IAN_HOLIDAY: &str = "Ian's Birthday";
pub const HOTDOG_DAY: &str = "National Hot Dog Day";
pub const ICE_CREAM_DAY: &str = "National Ice Cream Day";
pub const CHERNOBYL_ANNIVERSARY: &str = "Chernobyl Disaster Remembrance Day";
pub const LIZARD_ATRAKOR_DAY: &str = "Festival of Atrakor's Might";
pub const MOTH_FLEET_DAY: &str = "Fleet Day";
pub const BEE_DAY: &str = "Bee Day";
pub const BEER_DAY: &str = "Beer Day";
pub const VEGAN_DAY: &str = "Vegan Day";
pub const TALK_LIKE_A_PIRATE_DAY: &str = "Talk-Like-a-Pirate Day";
pub const ST_PATRICK_DAY: &str = "St. Patrick's Day";

/// ISO weekdays.
pub const MONDAY: u32 = 1;
pub const THURSDAY: u32 = 4;
pub const FRIDAY: u32 = 5;
pub const SATURDAY: u32 = 6;
pub const SUNDAY: u32 = 7;

/// `TIMEZONE_LINT`, `TIMEZONE_UTC`, `TIMEZONE_ANYWHERE_ON_EARTH`: the default 50 hour window.
const DEFAULT_TZ: &[f64] = &[14.0, 0.0, -12.0];
/// NZDT, CHADT.
const NZ_TZ: &[f64] = &[13.0, 13.75];
/// TKT, TOT, NZST, NFT, LHST, AEST, ACST, ACWST, AWST, CXT, CCT, CKT, NUT.
const ANZ_TZ: &[f64] = &[
    13.0, 13.0, 12.0, 11.0, 10.5, 10.0, 9.5, 8.75, 8.0, 7.0, 6.5, -10.0, -11.0,
];
/// EDT, CDT, MDT, MST, PDT, AKDT, HDT, HST.
const USA_TZ: &[f64] = &[-4.0, -5.0, -6.0, -7.0, -7.0, -8.0, -9.0, -10.0];
/// CEST.
const FRANCE_TZ: &[f64] = &[2.0];
/// EST, CST, MST, PST, AKST, HST (both Thanksgivings).
const THANKSGIVING_TZ: &[f64] = &[-5.0, -6.0, -7.0, -8.0, -9.0, -10.0];

/// When a holiday is celebrated. Days and months are `(day, month)`; an end of `0` is the
/// beginning (tg `end_day`/`end_month` defaults).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum When {
    Dates {
        begin: (u32, u32),
        end: (u32, u32),
    },
    /// `/datum/holiday/nth_week`: from the `begin.0`th `begin.1` weekday to the `end.0`th `end.1`
    /// one of `month`. `moth`: Moth Week's extra week when the month starts on a Friday or later.
    NthWeek {
        month: u32,
        begin: (u32, u32),
        end: (u32, u32),
        moth: bool,
    },
    FridayThirteenth,
    /// The 256th day of the year.
    Programmers,
    /// Easter Sunday, a day early and a day extra.
    Easter,
    /// Dates of the Islamic calendar.
    Islamic {
        begin: (u32, u32),
        end: (u32, u32),
    },
    /// Dates of the Hebrew calendar (Nisan is month 1, Tishri 7).
    Hebrew {
        begin: (u32, u32),
        end: (u32, u32),
    },
}

/// A `/datum/holiday`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HolidayDef {
    pub name: &'static str,
    pub when: When,
    /// Hours from UTC (tg `timezones`).
    pub timezones: &'static [f64],
}

const fn day(name: &'static str, d: u32, m: u32) -> HolidayDef {
    days(name, (d, m), (0, 0))
}

const fn days(name: &'static str, begin: (u32, u32), end: (u32, u32)) -> HolidayDef {
    HolidayDef {
        name,
        when: When::Dates { begin, end },
        timezones: DEFAULT_TZ,
    }
}

const fn zoned(def: HolidayDef, timezones: &'static [f64]) -> HolidayDef {
    HolidayDef { timezones, ..def }
}

const fn moving(name: &'static str, when: When) -> HolidayDef {
    HolidayDef {
        name,
        when,
        timezones: DEFAULT_TZ,
    }
}

const fn nth(name: &'static str, week: u32, weekday: u32, month: u32) -> HolidayDef {
    moving(
        name,
        When::NthWeek {
            month,
            begin: (week, weekday),
            end: (week, weekday),
            moth: false,
        },
    )
}

/// Every holiday, tg's order.
pub const HOLIDAYS: &[HolidayDef] = &[
    // January
    day(MOTH_FLEET_DAY, 19, 1),
    // February
    day("Groundhog Day", 2, 2),
    zoned(day("Waitangi Day", 6, 2), NZ_TZ),
    days(VALENTINES, (13, 2), (15, 0)),
    day("Birthday of Space Station 13", 16, 2),
    day("Random Acts of Kindness Day", 17, 2),
    day("Leap Day", 29, 2),
    // March
    day("Pi Day", 14, 3),
    day(ST_PATRICK_DAY, 17, 3),
    // April
    days(APRIL_FOOLS, (1, 4), (2, 0)),
    day("Cosmonautics Day", 12, 4),
    day("Four-Twenty", 20, 4),
    day("National Tea Day", 21, 4),
    day("Earth Day", 22, 4),
    zoned(day("ANZAC Day", 25, 4), ANZ_TZ),
    day(CHERNOBYL_ANNIVERSARY, 26, 4),
    // May
    day("Labor Day", 1, 5),
    day("Draconic Language Day", 3, 5),
    day("Firefighter's Day", 4, 5),
    day(BEE_DAY, 20, 5),
    day("Goth Day", 22, 5),
    // June
    day(LIZARD_ATRAKOR_DAY, 15, 6),
    day(GARBAGEDAY, 17, 6),
    day("Summer Solstice", 21, 6),
    // July
    day("Doctor's Day", 1, 7),
    day("UFO Day", 2, 7),
    zoned(day("US Independence Day", 4, 7), USA_TZ),
    day("Writer's Day", 8, 7),
    zoned(day("Bastille Day", 14, 7), FRANCE_TZ),
    day(HOTDOG_DAY, 17, 7),
    day("Wizard's Day", 27, 7),
    day("Friendship Day", 30, 7),
    // August
    day("International Day of the World's Indigenous Peoples", 9, 8),
    day("Independence Day of Ukraine", 24, 8),
    // September
    day("Tiziran Unification Day", 1, 9),
    days(IAN_HOLIDAY, (9, 9), (10, 0)),
    day(TALK_LIKE_A_PIRATE_DAY, 19, 9),
    day("Stupid-Questions Day", 28, 9),
    // October
    day("Animal's Day", 4, 10),
    day("Smiling Day", 7, 10),
    day("Boss' Day", 16, 10),
    day(
        "Anniversary of the Foundation of the United Nations",
        24,
        10,
    ),
    days(HALLOWEEN, (29, 10), (2, 11)),
    // November
    day(VEGAN_DAY, 1, 11),
    days("October Revolution", (6, 11), (7, 0)),
    day("Remembrance Day", 11, 11),
    day("Life Day", 17, 11),
    day("Kindness Day", 13, 11),
    day("Flowers Day", 19, 11),
    day("Saying-'Hello' Day", 21, 11),
    day("Festival of Holy Lights", 28, 11),
    // December
    days(FESTIVE_SEASON, (1, 12), (31, 0)),
    day("Human-Rights Day", 10, 12),
    day(MONKEYDAY, 14, 12),
    day("Mayan Doomsday Anniversary", 21, 12),
    days(CHRISTMAS, (18, 12), (27, 0)),
    day("Boxing Day", 26, 12),
    days(NEW_YEAR, (31, 12), (2, 1)),
    // Moving dates
    moving("Friday the 13th", When::FridayThirteenth),
    moving("Programmers' Day", When::Programmers),
    moving(
        "Start of Ramadan",
        When::Islamic {
            begin: (1, 9),
            end: (3, 0),
        },
    ),
    moving(
        "End of Ramadan",
        When::Islamic {
            begin: (28, 9),
            end: (1, 10),
        },
    ),
    moving(
        "Hanukkah",
        When::Hebrew {
            begin: (25, 9),
            end: (2, 10),
        },
    ),
    moving(
        "Passover",
        When::Hebrew {
            begin: (15, 1),
            end: (22, 0),
        },
    ),
    moving(EASTER, When::Easter),
    // nth_week.dm
    zoned(
        nth("Thanksgiving in the United States", 4, THURSDAY, 11),
        THANKSGIVING_TZ,
    ),
    zoned(
        nth("Thanksgiving in Canada", 2, MONDAY, 10),
        THANKSGIVING_TZ,
    ),
    nth("Mother's Day", 2, SUNDAY, 5),
    nth("Father's Day", 3, SUNDAY, 6),
    nth(BEER_DAY, 1, FRIDAY, 8),
    moving(
        MOTH_WEEK,
        When::NthWeek {
            month: 7,
            begin: (3, SATURDAY),
            end: (4, SUNDAY),
            moth: true,
        },
    ),
    nth(ICE_CREAM_DAY, 3, SUNDAY, 7),
];

/// A Gregorian date with its ISO weekday (1 = Monday).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub weekday: u32,
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Date {
    /// The date `days` after 1970-01-01 (`civil_from_days`).
    pub fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        // 1970-01-01 was a Thursday.
        let weekday = (days + 3).rem_euclid(7) as u32 + 1;
        Self {
            year,
            month,
            day,
            weekday,
        }
    }

    pub fn ymd(year: i64, month: u32, day: u32) -> Self {
        Self::from_days(days_from_civil(year, month, day))
    }

    pub fn days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// The date in the timezone `hours` from UTC at Unix time `secs`.
    pub fn at(secs: i64, hours: f64) -> Self {
        let local = secs + (hours * 3600.0) as i64;
        Self::from_days(local.div_euclid(86_400))
    }
}

/// tg `/datum/holiday/shouldCelebrate` on plain dates.
fn in_dates(dd: u32, mm: u32, begin: (u32, u32), end: (u32, u32)) -> bool {
    let (bd, bm) = begin;
    let ed = if end.0 == 0 { bd } else { end.0 };
    let em = if end.1 == 0 { bm } else { end.1 };
    if em > bm {
        // Spans months of one year.
        if mm == em {
            dd <= ed
        } else if mm == bm {
            dd >= bd
        } else {
            (bm..=em).contains(&mm)
        }
    } else if em == bm {
        mm == bm && (bd..=ed).contains(&dd)
    } else {
        // Starts in one year, ends in the next.
        (mm >= bm && dd >= bd) || (mm <= em && dd <= ed)
    }
}

/// The day of the month of the `week`th `weekday` (`nth_week/shouldCelebrate`).
fn nth_weekday(year: i64, month: u32, week: u32, weekday: u32) -> u32 {
    let first = Date::ymd(year, month, 1).weekday as i64;
    let mut d = i64::from(weekday) - first;
    if d < 0 {
        d += 7;
    }
    (d + i64::from(week - 1) * 7 + 1) as u32
}

/// Easter Sunday of `year` (the anonymous Gregorian computus; tg's `EasterDate` tables give
/// the same dates).
pub fn easter(year: i64) -> (u32, u32) {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    (day as u32, month as u32)
}

/// tg `shouldCelebrate(dd, mm, yyyy, ddd)` of `def` on `date`.
pub fn should_celebrate(def: &HolidayDef, date: Date) -> bool {
    let (dd, mm) = (date.day, date.month);
    match def.when {
        When::Dates { begin, end } => in_dates(dd, mm, begin, end),
        When::NthWeek {
            month,
            begin,
            end,
            moth,
        } => {
            // National Moth Week is the last full week of July: a month starting on a Friday or
            // later makes week 5 a full week.
            let extra = u32::from(moth && Date::ymd(date.year, month, 1).weekday >= FRIDAY);
            let bd = nth_weekday(date.year, month, begin.0 + extra, begin.1);
            let ed = nth_weekday(date.year, month, end.0 + extra, end.1);
            in_dates(dd, mm, (bd, month), (ed, month))
        }
        When::FridayThirteenth => dd == 13 && date.weekday == FRIDAY,
        // tg's `yyyy/4 == round(yyyy/4)`: every fourth year is a leap year.
        When::Programmers => {
            let day = if date.year % 4 == 0 { 12 } else { 13 };
            mm == 9 && dd == day
        }
        When::Easter => {
            let (d, m) = easter(date.year);
            let sunday = days_from_civil(date.year, m, d);
            (sunday - 1..=sunday + 1).contains(&date.days())
        }
        When::Islamic { begin, end } => {
            let (_, m, d) = calendar::islamic(date.year, date.month, date.day);
            in_dates(d, m, begin, end)
        }
        When::Hebrew { begin, end } => {
            let (_, m, d) = calendar::hebrew(date.year, date.month, date.day);
            in_dates(d, m, begin, end)
        }
    }
}

/// Whether `def` is celebrated at Unix time `secs` in any of its timezones.
pub fn celebrated(def: &HolidayDef, secs: i64) -> bool {
    def.timezones
        .iter()
        .any(|&tz| should_celebrate(def, Date::at(secs, tz)))
}

/// tg `foreign_calendar.dm` (John Walker's fourmilab conversions), on Julian days.
pub mod calendar {
    const HEBREW_EPOCH: f64 = 347_995.5;
    const ISLAMIC_EPOCH: f64 = 1_948_439.5;
    /// Fourmilab's value. tg's `1721424.5` puts every Julian day one day early, so its Islamic
    /// and Hebrew holidays came a day late.
    const GREGORIAN_EPOCH: f64 = 1_721_425.5;

    fn leap_gregorian(year: i64) -> bool {
        year % 4 == 0 && (year % 400 == 0 || year % 100 != 0)
    }

    pub fn gregorian_to_jd(year: i64, month: u32, day: u32) -> f64 {
        let (y, m) = (year as f64, f64::from(month));
        let mut jd = f64::from(day)
            + (GREGORIAN_EPOCH - 1.0)
            + 365.0 * (y - 1.0)
            + ((367.0 * m - 362.0) / 12.0).floor()
            + ((y - 1.0) / 4.0).floor()
            - ((y - 1.0) / 100.0).floor()
            + ((y - 1.0) / 400.0).floor();
        if month > 2 {
            jd -= if leap_gregorian(year) { 1.0 } else { 2.0 };
        }
        jd
    }

    fn islamic_to_jd(year: i64, month: u32, day: u32) -> f64 {
        let y = year as f64;
        f64::from(day)
            + (29.5 * f64::from(month - 1)).ceil()
            + (y - 1.0) * 354.0
            + ((3.0 + 11.0 * y) / 30.0).floor()
            + ISLAMIC_EPOCH
            - 1.0
    }

    /// The Islamic (year, month, day) of a Gregorian date.
    pub fn islamic(year: i64, month: u32, day: u32) -> (i64, u32, u32) {
        let jd = gregorian_to_jd(year, month, day);
        let jd_adj = jd.floor() + 0.5;
        let yyyy = ((30.0 * (jd_adj - ISLAMIC_EPOCH) + 10646.0) / 10631.0).floor() as i64;
        let mm = ((jd - (29.0 + islamic_to_jd(yyyy, 1, 1))) / 29.5 + 1.0)
            .ceil()
            .clamp(1.0, 12.0) as u32;
        let dd = (jd - islamic_to_jd(yyyy, mm, 1) + 1.0).round() as u32;
        (yyyy, mm, dd)
    }

    fn hebrew_leap(year: i64) -> bool {
        matches!(year.rem_euclid(19), 0 | 3 | 6 | 8 | 11 | 14 | 17)
    }

    fn hebrew_year_months(year: i64) -> u32 {
        if hebrew_leap(year) { 13 } else { 12 }
    }

    fn hebrew_delay_1(year: i64) -> i64 {
        let months = (235 * year - 234).div_euclid(19);
        let parts = 12084 + 13753 * months;
        let mut day = months * 29 + parts.div_euclid(25920);
        if (3 * (day + 1)).rem_euclid(7) < 3 {
            day += 1;
        }
        day
    }

    fn hebrew_delay_2(year: i64) -> i64 {
        let last = hebrew_delay_1(year - 1);
        let present = hebrew_delay_1(year);
        let next = hebrew_delay_1(year + 1);
        if next - present == 356 {
            2
        } else if present - last == 382 {
            1
        } else {
            0
        }
    }

    fn hebrew_year_days(year: i64) -> f64 {
        hebrew_to_jd(year + 1, 7, 1) - hebrew_to_jd(year, 7, 1)
    }

    fn hebrew_month_days(year: i64, month: u32) -> u32 {
        match month {
            2 | 4 | 6 | 10 | 13 => 29,
            12 if !hebrew_leap(year) => 29,
            8 if hebrew_year_days(year) as i64 % 10 != 5 => 29,
            9 if hebrew_year_days(year) as i64 % 10 == 3 => 29,
            _ => 30,
        }
    }

    fn hebrew_to_jd(year: i64, month: u32, day: u32) -> f64 {
        let months = hebrew_year_months(year);
        let mut jd = HEBREW_EPOCH
            + (hebrew_delay_1(year) + hebrew_delay_2(year)) as f64
            + f64::from(day)
            + 1.0;
        if month < 7 {
            for mon in 7..=months {
                jd += f64::from(hebrew_month_days(year, mon));
            }
            for mon in 1..month {
                jd += f64::from(hebrew_month_days(year, mon));
            }
        } else {
            for mon in 7..month {
                jd += f64::from(hebrew_month_days(year, mon));
            }
        }
        jd
    }

    /// The Hebrew (year, month, day) of a Gregorian date.
    pub fn hebrew(year: i64, month: u32, day: u32) -> (i64, u32, u32) {
        let jd = gregorian_to_jd(year, month, day).floor() + 0.5;
        let count = ((jd - HEBREW_EPOCH) * 98496.0 / 35_975_351.0).floor() as i64;
        let mut year = count - 1;
        let mut i = count;
        while jd >= hebrew_to_jd(i, 7, 1) {
            year += 1;
            i += 1;
        }
        let mut month = if jd < hebrew_to_jd(year, 1, 1) { 7 } else { 1 };
        let mut i = month;
        // A month past the year's last stops the walk (tg has no bound).
        while i <= 13 && jd > hebrew_to_jd(year, i, hebrew_month_days(year, i)) {
            month += 1;
            i += 1;
        }
        let day = (jd - hebrew_to_jd(year, month, 1) + 1.0).round() as u32;
        (year, month, day)
    }
}

/// tg config `ALLOW_HOLIDAYS` (on in BandaStation's `config.txt`). Insert before
/// `GameServerPlugin` to override; otherwise from the environment:
/// - `SS13_HOLIDAYS=0`: no holidays.
/// - `SS13_HOLIDAY_DATE=YYYY-MM-DD` (debug builds only): celebrate that date (noon UTC).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct HolidaysConfig {
    pub allow: bool,
    /// Unix seconds to celebrate; `None`: now.
    pub at: Option<i64>,
}

impl HolidaysConfig {
    /// Test servers: no holidays, so the calendar never changes a test.
    pub fn none() -> Self {
        Self {
            allow: false,
            at: None,
        }
    }

    /// Holidays as on `date` (noon UTC).
    pub fn on(year: i64, month: u32, day: u32) -> Self {
        Self {
            allow: true,
            at: Some(days_from_civil(year, month, day) * 86_400 + 43_200),
        }
    }
}

impl Default for HolidaysConfig {
    fn default() -> Self {
        let at = std::env::var("SS13_HOLIDAY_DATE")
            .ok()
            .filter(|_| cfg!(debug_assertions))
            .and_then(|v| parse_date(&v))
            .map(|(y, m, d)| days_from_civil(y, m, d) * 86_400 + 43_200);
        Self {
            allow: std::env::var("SS13_HOLIDAYS").map_or(true, |v| v != "0"),
            at,
        }
    }
}

/// `YYYY-MM-DD`.
fn parse_date(text: &str) -> Option<(i64, u32, u32)> {
    let mut parts = text.trim().splitn(3, '-');
    let y = parts.next()?.parse::<i64>().ok()?;
    let m = parts
        .next()?
        .parse::<u32>()
        .ok()
        .filter(|m| (1..=12).contains(m))?;
    let d = parts
        .next()?
        .parse::<u32>()
        .ok()
        .filter(|d| (1..=31).contains(d))?;
    Some((y, m, d))
}

/// tg `GLOB.holidays`: today's holidays, shuffled.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Holidays {
    pub active: Vec<&'static HolidayDef>,
    /// The year of the date celebrated (greetings count years with it).
    pub year: i64,
}

impl Holidays {
    pub fn has(&self, name: &str) -> bool {
        self.active.iter().any(|h| h.name == name)
    }
}

/// tg `check_holidays(name)`.
pub fn check_holidays(world: &World, name: &str) -> bool {
    world
        .get_resource::<Holidays>()
        .is_some_and(|h| h.has(name))
}

/// tg `fill_holidays()`: today's holidays into [`Holidays`], shuffled with the event stream.
pub(super) fn fill(world: &mut World) {
    let config = world
        .get_resource::<HolidaysConfig>()
        .cloned()
        .unwrap_or_default();
    if !config.allow {
        world.insert_resource(Holidays::default());
        return;
    }
    let secs = config.at.unwrap_or_else(|| {
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    });
    let mut active: Vec<&'static HolidayDef> =
        HOLIDAYS.iter().filter(|h| celebrated(h, secs)).collect();
    if let Some(mut rng) = world.get_resource_mut::<EventRng>() {
        for i in (1..active.len()).rev() {
            let j = rng.0.range(0, i as i64) as usize;
            active.swap(i, j);
        }
    }
    if !active.is_empty() {
        let names: Vec<&str> = active.iter().map(|h| h.name).collect();
        info!("holidays: {names:?}");
    }
    world.insert_resource(Holidays {
        active,
        year: Date::at(secs, 0.0).year,
    });
}

/// The control vars holidays change (`/datum/round_event_control/radiation_storm/New`).
pub(super) fn adjust_controls(controls: &mut [EventControl], holidays: &Holidays) {
    if !holidays.has(CHERNOBYL_ANNIVERSARY) {
        return;
    }
    for c in controls
        .iter_mut()
        .filter(|c| c.name == super::radiation_storm::NAME)
    {
        if c.max_occurrences > 0 && c.weight > 0 {
            c.weight *= 2;
            c.max_occurrences += 2;
        }
    }
}

/// tg `/datum/holiday/proc/greet()`.
pub fn greeting(def: &HolidayDef, year: i64, rng: &mut EventRng) -> String {
    let name = def.name;
    match name {
        MOTH_FLEET_DAY => "This day commemorates another year of successful survival aboard the \
             Mothic Grand Nomad Fleet. Moths galaxywide are encouraged to eat, drink, and be merry."
            .into(),
        "Waitangi Day" => format!(
            "On this day {} years ago, New Zealand's Treaty of Waitangi, the founding document of \
             the nation, was signed!",
            year - 1840
        ),
        "Birthday of Space Station 13" => {
            let age = year - 2003;
            let fact = match age {
                16 => " SS13 is now old enough to drive!".to_owned(),
                18 => " SS13 is now legal!".to_owned(),
                21 => " SS13 can now drink!".to_owned(),
                26 => " SS13 can now rent a car!".to_owned(),
                30 => " SS13 can now go home and be a family man!".to_owned(),
                35 => " SS13 can now run for President of the United States!".to_owned(),
                40 => " SS13 can now suffer a midlife crisis!".to_owned(),
                50 => " Happy golden anniversary!".to_owned(),
                65 => " SS13 can now start thinking about retirement!".to_owned(),
                _ => format!(" SS13 is now {age} years old!"),
            };
            format!(
                "Say 'Happy Birthday' to Space Station 13, first publicly playable on February \
                 16th, 2003!{fact}"
            )
        }
        "Random Acts of Kindness Day" => {
            "Go do some random acts of kindness for a stranger!".into()
        }
        ST_PATRICK_DAY => "Happy National Inebriation Day!".into(),
        "Cosmonautics Day" => {
            "On this day over 600 years ago, Comrade Yuri Gagarin first ventured into space!".into()
        }
        CHERNOBYL_ANNIVERSARY => "On this day in 1986, the Chernobyl nuclear power plant melted \
             down, causing one of the worst nuclear disasters in human history. Today serves as a \
             reminder to the lives lost and to the rigorous safety standards our engineers must \
             adhere to when providing power to the station."
            .into(),
        "Draconic Language Day" => "On this day, Lizardkind celebrates their language with \
             literature and other cultural works."
            .into(),
        LIZARD_ATRAKOR_DAY => "On this day, the Lizards traditionally celebrate the Festival of \
             Atrakor's Might, where they honour the moon god with lavishly adorned clothing, large \
             portions of food, and a massive celebration into the night."
            .into(),
        "Bastille Day" => "Do you hear the people sing?".into(),
        HOTDOG_DAY => "Happy National Hot Dog Day!".into(),
        "Friendship Day" => format!("Have a magical {name}!"),
        "Tiziran Unification Day" => "On this day over 400 years ago, Lizardkind first united \
             under a single banner, ready to face the stars as one unified people."
            .into(),
        IAN_HOLIDAY => "Happy birthday, Ian!".into(),
        TALK_LIKE_A_PIRATE_DAY => {
            "Ye be talkin' like a pirate today or else ye'r walkin' tha plank, matey!".into()
        }
        "Stupid-Questions Day" => format!("Are you having a happy {name}?"),
        "Anniversary of the Foundation of the United Nations" => "On this day in 1945, the United \
             Nations was founded, laying the foundation for humanity's united government!"
            .into(),
        HALLOWEEN => "Have a spooky Halloween!".into(),
        "Remembrance Day" => "Lest we forget.".into(),
        "Saying-'Hello' Day" => {
            const HELLOS: [&str; 13] = [
                "Aloha",
                "Bonjour",
                "Hello",
                "Hi",
                "Greetings",
                "Salutations",
                "Bienvenidos",
                "Hola",
                "Howdy",
                "Ni hao",
                "Guten Tag",
                "Konnichiwa",
                "G'day cunt",
            ];
            let hello = rng.pick(&HELLOS).copied().unwrap_or("Hello");
            format!("{hello}! Have a happy {name}!")
        }
        "Festival of Holy Lights" => "The Festival of Holy Lights is the final day of the \
             Ethereal calendar. It is typically a day of prayer followed by celebration to close \
             out the year in style."
            .into(),
        FESTIVE_SEASON => "Have a nice festive season!".into(),
        CHRISTMAS => "Have a merry Christmas!".into(),
        "Hanukkah" => format!(
            "Happy {}!",
            rng.pick(&["Hanukkah", "Chanukah"])
                .copied()
                .unwrap_or("Hanukkah")
        ),
        EASTER => "Greetings! Have a Happy Easter and keep an eye out for Easter Bunnies!".into(),
        "Mother's Day" => "Happy Mother's Day in most of the Americas, Asia, and Oceania!".into(),
        _ => format!("Have a happy {name}!"),
    }
}

/// `SSticker/setup` after the welcome: "и..." and every holiday's greeting, to everyone.
pub fn greet(world: &mut World) {
    let Some(holidays) = world.get_resource::<Holidays>().cloned() else {
        return;
    };
    if holidays.active.is_empty() {
        return;
    }
    let clients: Vec<Entity> = world
        .query_filtered::<Entity, With<ClientToken>>()
        .iter(world)
        .collect();
    let mut lines = vec![(Span::Notice, "и...".to_owned())];
    world.try_resource_scope(|_, mut rng: Mut<EventRng>| {
        for def in &holidays.active {
            lines.push((Span::Info, greeting(def, holidays.year, &mut rng)));
        }
    });
    for (span, text) in lines {
        world.write_message(SendChat {
            clients: clients.clone(),
            message: ChatMessage::simple(ChatChannel::System, span, text),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> &'static HolidayDef {
        HOLIDAYS.iter().find(|h| h.name == name).expect(name)
    }

    fn on(name: &str, y: i64, m: u32, d: u32) -> bool {
        should_celebrate(def(name), Date::ymd(y, m, d))
    }

    #[test]
    fn dates_and_weekdays() {
        assert_eq!(
            Date::from_days(0),
            Date {
                year: 1970,
                month: 1,
                day: 1,
                weekday: THURSDAY
            }
        );
        let d = Date::ymd(2026, 11, 6);
        assert_eq!((d.year, d.month, d.day, d.weekday), (2026, 11, 6, FRIDAY));
        assert_eq!(
            Date::ymd(2024, 2, 29).days() + 1,
            Date::ymd(2024, 3, 1).days()
        );
        // Kiribati is already in tomorrow, Baker Island still in yesterday.
        let noon = days_from_civil(2026, 1, 1) * 86_400 + 12 * 3600 - 3600;
        assert_eq!(Date::at(noon, 14.0).day, 2);
        assert_eq!(Date::at(noon, -12.0).day, 31);
    }

    #[test]
    fn fixed_dates_and_year_wraps() {
        assert!(on(VALENTINES, 2026, 2, 13) && on(VALENTINES, 2026, 2, 15));
        assert!(!on(VALENTINES, 2026, 2, 16));
        assert!(on(HALLOWEEN, 2026, 10, 29) && on(HALLOWEEN, 2026, 11, 2));
        assert!(!on(HALLOWEEN, 2026, 11, 3) && !on(HALLOWEEN, 2026, 10, 28));
        assert!(on(NEW_YEAR, 2026, 12, 31) && on(NEW_YEAR, 2027, 1, 2));
        assert!(!on(NEW_YEAR, 2027, 1, 3));
        assert!(on(FESTIVE_SEASON, 2026, 12, 1) && on(FESTIVE_SEASON, 2026, 12, 31));
        assert!(on("October Revolution", 2026, 11, 6));
        assert!(!HOLIDAYS.iter().any(|h| h.name == "Pride Week"));
    }

    #[test]
    fn moving_dates() {
        // 2026-11-26 is the fourth Thursday of November.
        let us = "Thanksgiving in the United States";
        assert!(on(us, 2026, 11, 26) && !on(us, 2026, 11, 19));
        assert!(on("Thanksgiving in Canada", 2026, 10, 12));
        assert!(on("Mother's Day", 2026, 5, 10));
        assert!(on("Friday the 13th", 2026, 11, 13) && !on("Friday the 13th", 2026, 12, 13));
        assert!(on("Programmers' Day", 2024, 9, 12) && on("Programmers' Day", 2026, 9, 13));
        // July 2026 starts on a Wednesday: the 3rd Saturday (18th) to the 4th Sunday (26th).
        assert!(on(MOTH_WEEK, 2026, 7, 18) && on(MOTH_WEEK, 2026, 7, 26));
        assert!(!on(MOTH_WEEK, 2026, 7, 27));
        // July 2022 started on a Friday: a week later.
        assert!(!on(MOTH_WEEK, 2022, 7, 16) && on(MOTH_WEEK, 2022, 7, 23));
        assert_eq!(easter(2026), (5, 4));
        assert_eq!(easter(2025), (20, 4));
        assert!(on(EASTER, 2026, 4, 4) && on(EASTER, 2026, 4, 6) && !on(EASTER, 2026, 4, 7));
    }

    #[test]
    fn foreign_calendars() {
        // Ramadan 1447 began on 2026-02-18; Hanukkah 5786 (25 Kislev) on 2025-12-15.
        assert_eq!(calendar::islamic(2026, 2, 18).1, 9);
        assert!(on("Start of Ramadan", 2026, 2, 19));
        assert!(!on("Start of Ramadan", 2026, 3, 30));
        assert_eq!(calendar::hebrew(2025, 12, 15), (5786, 9, 25));
        assert!(on("Hanukkah", 2025, 12, 16) && !on("Hanukkah", 2025, 11, 16));
    }

    #[test]
    fn chernobyl_doubles_radiation_storms() {
        let mut controls = crate::events::controls();
        let holidays = Holidays {
            active: vec![def(CHERNOBYL_ANNIVERSARY)],
            year: 2026,
        };
        let storm = |c: &[EventControl]| {
            c.iter()
                .find(|c| c.name == crate::events::radiation_storm::NAME)
                .map(|c| (c.weight, c.max_occurrences))
                .unwrap()
        };
        let before = storm(controls.as_slice());
        adjust_controls(&mut controls, &holidays);
        assert_eq!(storm(controls.as_slice()), (before.0 * 2, before.1 + 2));
    }

    #[test]
    fn config_dates_parse() {
        assert_eq!(parse_date("2026-12-25"), Some((2026, 12, 25)));
        assert_eq!(parse_date("2026-13-01"), None);
        let config = HolidaysConfig::on(2026, 12, 25);
        let secs = config.at.unwrap();
        assert!(celebrated(def(CHRISTMAS), secs));
        assert!(!celebrated(def(HALLOWEEN), secs));
    }
}
