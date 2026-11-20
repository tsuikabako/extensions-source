//! The site's relative upload times ("5d ago") as dates. Taken unchanged from the first Comix plug-in (`parked/comix` in the history),
//! itself a port of the keiyoushi extension's `parseRelativeDate` (Apache-2.0).

/// Kotlin `parseRelativeDate`: "5d ago" (or "5d") is `now` minus that span, in epoch milliseconds. Units: s, sec(s), m,
/// min(s), h, hr(s), d, day(s), w, week(s), mo, mos, month(s), y, yr(s), year(s); anything else, and an empty text, is 0.
/// `now` is the host clock in Unix seconds. Months and years move the calendar in UTC and clamp the day, as Calendar.add
/// does. ponytail: the other units are fixed spans of UTC seconds, while the Kotlin uses the device zone, which differs
/// only across a daylight-saving change.
pub fn upload_date(text: &str, now: i64) -> i64 {
    let lower = text.trim().to_lowercase();
    let body = lower.strip_suffix(" ago").unwrap_or(lower.as_str());
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let Ok(amount) = body[..digits].parse::<i32>() else { return 0 };
    let amount = i64::from(amount);
    let seconds_per = match body[digits..].trim_start() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 3_600,
        "d" | "day" | "days" => 86_400,
        "w" | "week" | "weeks" => 7 * 86_400,
        "mo" | "mos" | "month" | "months" => return shift_months(now, amount),
        "y" | "yr" | "yrs" | "year" | "years" => return shift_months(now, 12 * amount),
        _ => return 0,
    };
    (now - seconds_per * amount) * 1_000
}

/// `now` moved back `months` calendar months in UTC: the day clamped to the length of the new month, the time of day
/// kept. Result in epoch milliseconds.
fn shift_months(now: i64, months: i64) -> i64 {
    let days = now.div_euclid(86_400);
    let time = now.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let index = year * 12 + i64::from(month) - 1 - months;
    let (year, month) = (index.div_euclid(12), index.rem_euclid(12) as u32 + 1);
    let day = day.min(days_in_month(year, month));
    (days_from_civil(year, month, day) * 86_400 + time) * 1_000
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The (year, month, day) of a count of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
