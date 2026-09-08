//! Dates and paths, as the interface says them.
//!
//! Two rules from board 06, and one dependency decision.
//!
//! **Durations are relative, instants are absolute.** "il y a 2 h" answers "is
//! this recent?" without the reader doing arithmetic; "14 mars 2026" answers
//! "when exactly?" without them doing it either. The card uses each where it
//! means something, and the choice is made here rather than at the call sites.
//!
//! **Local time is the user's, not the commit's.** A commit carries its
//! author's UTC offset, which is right for showing what their clock said and
//! wrong for "aujourd'hui". Anything phrased against *now* is resolved in the
//! system zone.
//!
//! That last rule is why `jiff` is a dependency. It was already in the tree
//! under `gix`, so it costs no build time and adds no licence, and the
//! alternative — deriving the system's UTC offset by hand, with its transitions
//! — is the kind of code that is quietly wrong for a year. `omagit-git` still
//! has no date dependency: it hands back seconds and an offset
//! (`omagit_git::Time::civil`), which is what keeps the Git core free of a
//! locale.

use std::path::Path;

use jiff::{Timestamp, Unit, Zoned, tz::TimeZone};

/// `/home/u/src/omagit` → `~/src/omagit`.
///
/// The home prefix is noise in a list where every row starts with it, and its
/// absence is information: a repository outside `$HOME` is worth noticing.
pub fn tildify(path: &Path) -> String {
    let text = path.display().to_string();
    let Some(home) = std::env::var_os("HOME") else {
        return text;
    };
    let home = Path::new(&home);
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => text,
    }
}

/// When the user last opened this repository: `aujourd'hui 09:14`,
/// `hier 18:02`, `14 mars 2026`.
pub fn opened(seconds: i64) -> String {
    opened_at(seconds, now())
}

/// How long ago something happened: `à l'instant`, `il y a 12 min`,
/// `il y a 2 h`, `il y a 3 j`, `il y a 5 mois`.
pub fn ago(seconds: i64) -> String {
    ago_between(seconds, now().timestamp().as_second())
}

fn now() -> Zoned {
    Timestamp::now().to_zoned(TimeZone::system())
}

/// Split from [`opened`] so the clock can be supplied by a test.
fn opened_at(seconds: i64, now: Zoned) -> String {
    let Ok(stamp) = Timestamp::from_second(seconds) else {
        return "date invalide".to_owned();
    };
    let then = stamp.to_zoned(now.time_zone().clone());
    let days = now
        .date()
        .since((Unit::Day, then.date()))
        .map(|span| span.get_days())
        .unwrap_or(0);
    let clock = format!("{:02}:{:02}", then.hour(), then.minute());
    match days {
        0 => format!("aujourd'hui {clock}"),
        1 => format!("hier {clock}"),
        _ if then.year() == now.year() => {
            format!("{} {}", then.day(), month(then.month()))
        }
        _ => format!("{} {} {}", then.day(), month(then.month()), then.year()),
    }
}

/// Split from [`ago`] for the same reason.
fn ago_between(seconds: i64, now: i64) -> String {
    let elapsed = now - seconds;
    // A commit dated in the future — a clock skew, a rebase — is not worth a
    // special phrasing, but "il y a -3 j" would be.
    if elapsed < 60 {
        return "à l'instant".to_owned();
    }
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    // Rounded months and years: past a few weeks, precision is not what the
    // reader wants, and "il y a 47 j" makes them do the division.
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;

    let (count, unit) = match elapsed {
        seconds if seconds < HOUR => (seconds / MINUTE, "min"),
        seconds if seconds < DAY => (seconds / HOUR, "h"),
        seconds if seconds < MONTH => (seconds / DAY, "j"),
        seconds if seconds < YEAR => (seconds / MONTH, "mois"),
        seconds => (seconds / YEAR, "an"),
    };
    // "mois" is already plural; "an" is not.
    let plural = if unit == "an" && count > 1 { "s" } else { "" };
    format!("il y a {count} {unit}{plural}")
}

fn month(number: i8) -> &'static str {
    match number {
        1 => "janvier",
        2 => "février",
        3 => "mars",
        4 => "avril",
        5 => "mai",
        6 => "juin",
        7 => "juillet",
        8 => "août",
        9 => "septembre",
        10 => "octobre",
        11 => "novembre",
        _ => "décembre",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed clock in a fixed zone, so the phrasing is the only variable.
    fn at(text: &str) -> Zoned {
        text.parse::<Timestamp>()
            .expect("a valid timestamp")
            .to_zoned(TimeZone::UTC)
    }

    #[test]
    fn says_today_and_yesterday_before_falling_back_to_a_date() {
        let now = at("2026-09-08T15:00:00Z");
        let seconds = |text: &str| text.parse::<Timestamp>().expect("valid").as_second();

        assert_eq!(
            opened_at(seconds("2026-09-08T09:14:00Z"), now.clone()),
            "aujourd'hui 09:14"
        );
        assert_eq!(
            opened_at(seconds("2026-09-07T18:02:00Z"), now.clone()),
            "hier 18:02"
        );
        // Earlier this year: the year is implied, so it is left out.
        assert_eq!(
            opened_at(seconds("2026-03-14T10:00:00Z"), now.clone()),
            "14 mars"
        );
        assert_eq!(
            opened_at(seconds("2025-03-14T10:00:00Z"), now),
            "14 mars 2025"
        );
    }

    #[test]
    fn a_day_boundary_is_a_calendar_day_not_twenty_four_hours() {
        // 23:50 yesterday is "hier", not "il y a 20 minutes" worth of today.
        let now = at("2026-09-08T00:10:00Z");
        let yesterday = "2026-09-07T23:50:00Z"
            .parse::<Timestamp>()
            .expect("valid")
            .as_second();
        assert_eq!(opened_at(yesterday, now), "hier 23:50");
    }

    #[test]
    fn scales_a_duration_to_the_unit_that_reads() {
        let now = 1_788_873_600;
        let ago = |elapsed: i64| ago_between(now - elapsed, now);

        assert_eq!(ago(5), "à l'instant");
        assert_eq!(ago(59), "à l'instant");
        assert_eq!(ago(60), "il y a 1 min");
        assert_eq!(ago(12 * 60), "il y a 12 min");
        assert_eq!(ago(2 * 3600), "il y a 2 h");
        assert_eq!(ago(3 * 86_400), "il y a 3 j");
        assert_eq!(ago(45 * 86_400), "il y a 1 mois");
        assert_eq!(ago(400 * 86_400), "il y a 1 an");
        assert_eq!(ago(800 * 86_400), "il y a 2 ans", "and 'an' takes an s");
    }

    #[test]
    fn a_timestamp_in_the_future_does_not_read_as_negative() {
        // Clock skew and rebases both produce them.
        let now = 1_788_873_600;
        assert_eq!(ago_between(now + 3600, now), "à l'instant");
    }

    #[test]
    fn shortens_a_path_under_home_and_leaves_the_rest_alone() {
        // SAFETY: this test owns the variable it sets, and no other test in
        // this module reads it.
        unsafe { std::env::set_var("HOME", "/home/tester") };
        assert_eq!(
            tildify(Path::new("/home/tester/src/omagit")),
            "~/src/omagit"
        );
        assert_eq!(tildify(Path::new("/home/tester")), "~");
        assert_eq!(
            tildify(Path::new("/mnt/work/repo")),
            "/mnt/work/repo",
            "a repository outside home keeps its full path, which is the point"
        );
    }
}
