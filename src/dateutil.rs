use chrono::{DateTime, Local, LocalResult, NaiveDateTime, ParseResult, TimeZone, Utc};

static GNUCASH_DT_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";
static GNUCASH_NO_DT_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/**
 * Attach noon, local time to a naive YMD date.
 */
pub fn localize_at_noon(ymd: &str) -> LocalResult<DateTime<Local>> {
    let noon: String = format!("{:}T12:00:00", ymd);
    let naive = NaiveDateTime::parse_from_str(&noon, "%Y-%m-%dT%H:%M:%S").unwrap();

    Local.from_local_datetime(&naive)
}

// In XML, datetimes are given with local TZ explicitly in them!
pub fn localize_from_dt_with_tz(datestring: &str) -> ParseResult<DateTime<Local>> {
    let dt = DateTime::parse_from_str(datestring, GNUCASH_DT_FORMAT)?;
    Ok(dt.with_timezone(&Local))
}

// In SQLite, all datetimes are UTC, but without timezone explicitly stated!
pub fn utc_to_datetime(datestring: &str) -> DateTime<Local> {
    let dt = NaiveDateTime::parse_from_str(datestring, GNUCASH_NO_DT_FORMAT).unwrap();
    let utc = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
    utc.with_timezone(&Local)
}

pub fn datetime_for_sqlite(dt: DateTime<Local>) -> String {
    let utc_dt: DateTime<Utc> = dt.into();
    utc_dt.format(GNUCASH_NO_DT_FORMAT).to_string()
}
