//! Finds exams: Canvas items whose titles say so, and dates stated in syllabus prose
//! ("Exam 2: Tuesday, 11/3…", "The midterm exams will take place on Mon, Sep 21, and
//! Mon, Oct 26", or a stacked "Exam 1 / Week of Sep 28 / during your lab session").
//! Errs on the side of missing an exam rather than inventing one: schedule tables,
//! review sessions and make-ups are ignored.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub struct FoundExam {
    pub label: String,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    /// Only the week is known ("Week of Sep 28" with no meeting to pin it to); `date` is its first day.
    pub approx: bool,
}

/// A class meeting from the syllabus header, e.g. LAB on W at 3:00 PM.
#[derive(Debug, Clone)]
struct Meeting {
    kind: String,
    days: Vec<Weekday>,
    start: Option<NaiveTime>,
}

const SKIP: &[&str] = &[
    "review",
    "make up",
    "make-up",
    "makeup",
    "practice",
    "study guide",
    "grade",
    "%",
    "weight",
    "drop",
    "exercise",
    "conflict",
    "average",
    "accommodat",
    "missed",
    "absence",
];

static KEYWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:(first|second|third|fourth|1st|2nd|3rd|4th)\s+)?(final\s+exam(?:ination)?|final|midterm\s+exams?|midterms?|exams?|tests?)(?:\s*#?\s*(\d))?\b")
        .unwrap()
});
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*\.?,?\s+)?(?:(\d{1,2})/(\d{1,2})(?:/(\d{2,4}))?|(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+(\d{1,2})(?:st|nd|rd|th)?\b(?:,?\s+(\d{4}))?)",
    )
    .unwrap()
});
static TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2})(?::(\d{2}))?\s*([ap])\.?\s*m\b\.?").unwrap());
static CLASS_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)start time:\s*(\d{1,2}):(\d{2})\s*([ap])m").unwrap());
static WEEK_OF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bweek of\s*$").unwrap());

/// Is this Canvas item an exam (rather than e.g. "Midterm review" or "Final project")?
pub fn is_exam_title(t: &str) -> bool {
    let l = t.to_lowercase();
    let Some(m) = KEYWORD.find(&l) else { return false };
    let after = &l[m.end()..];
    let not_exam = [
        "review",
        "practice",
        "project",
        "paper",
        "report",
        "presentation",
        "average",
        "study",
        "make",
        "grade",
        "reflection",
    ];
    !not_exam.iter().any(|w| l.contains(w))
        && !(m.as_str().trim() == "final" && after.trim_start().starts_with("draft"))
}

/// Exams stated in a syllabus. `year` fills in dates written without one.
pub fn from_syllabus(text: &str, year: i32) -> Vec<FoundExam> {
    // Scraped pages are full of non-breaking spaces ("Final\u{a0}Exam").
    let text = text.replace('\u{a0}', " ");
    let meetings = meetings(&text);
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out: Vec<FoundExam> = Vec::new();
    let push = |found: Vec<FoundExam>, out: &mut Vec<FoundExam>| {
        for e in found {
            if !out.iter().any(|o| o.date == e.date) {
                out.push(e);
            }
        }
    };
    for (i, line) in lines.iter().enumerate() {
        // Tables (tab-separated schedule rows) pair several dates with several topics.
        if line.contains('\t') {
            continue;
        }
        // Stacked layout: a bare label, then the date (and maybe a note) on the next lines.
        if is_label_line(line) {
            let ctx = stacked_context(&lines[i + 1..]);
            if DATE.is_match(&ctx) {
                push(parse_clause(&format!("{line}: {ctx}"), year, &meetings), &mut out);
                continue;
            }
        }
        for clause in clauses(line) {
            push(parse_clause(clause, year, &meetings), &mut out);
        }
    }
    out.sort_by_key(|e| (e.date, e.time));
    out
}

/// "Exam 1", "Final Exam", "Midterm 2" alone on a line — not a section heading
/// like "Midterm and Final Exams".
fn is_label_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    !line.is_empty()
        && line.split_whitespace().count() <= 4
        && !line.contains(':')
        && !lower.contains(" and ")
        && !lower.contains('&')
        && KEYWORD.is_match(line)
        && !DATE.is_match(line)
        && !SKIP.iter().any(|s| lower.contains(s))
}

/// The lines after a stacked label: up to the one with the date, plus one sentence
/// after it (e.g. "Take the exam during your lab session."). Stops at the next label
/// or at a heading-like line.
fn stacked_context(next: &[&str]) -> String {
    let mut parts = Vec::new();
    let mut seen_date = false;
    for (k, l) in next.iter().filter(|l| !l.is_empty()).take(3).enumerate() {
        let heading = l.split_whitespace().count() <= 5 && !l.ends_with('.') && !DATE.is_match(l);
        // The line right under a label should be the date itself, not a paragraph.
        if is_label_line(l) || heading || (k == 0 && l.split_whitespace().count() > 14) {
            break;
        }
        parts.push(*l);
        if seen_date {
            break;
        }
        seen_date |= DATE.is_match(l);
    }
    parts.join(" ")
}

/// Meeting blocks from the syllabus header (Simple Syllabus layout):
/// "Meeting Type: LAB / Meeting Days: W / Start Time: 3:00PM".
fn meetings(text: &str) -> Vec<Meeting> {
    let mut out: Vec<Meeting> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some((_, kind)) = line.split_once("Meeting Type:") {
            out.push(Meeting { kind: kind.trim().to_uppercase(), days: vec![], start: None });
        } else if let Some(m) = out.last_mut() {
            if let Some(days) = line.strip_prefix("Meeting Days:") {
                m.days = days
                    .trim()
                    .chars()
                    .filter_map(|c| match c {
                        'M' => Some(Weekday::Mon),
                        'T' => Some(Weekday::Tue),
                        'W' => Some(Weekday::Wed),
                        'R' => Some(Weekday::Thu),
                        'F' => Some(Weekday::Fri),
                        'S' => Some(Weekday::Sat),
                        'U' => Some(Weekday::Sun),
                        _ => None,
                    })
                    .collect();
            } else if m.start.is_none() {
                m.start = CLASS_START.captures(line).and_then(|c| to_time(&c[1], Some(&c[2]), &c[3]));
            }
        }
    }
    // Older syllabi: no meeting blocks, but maybe a lone "Start Time".
    if out.is_empty()
        && let Some(start) = CLASS_START.captures(text).and_then(|c| to_time(&c[1], Some(&c[2]), &c[3]))
    {
        out.push(Meeting { kind: "LEC".into(), days: vec![], start: Some(start) });
    }
    out
}

/// Which meeting a clause says the exam happens in ("during your lab", "during class time").
fn session<'a>(lower: &str, meetings: &'a [Meeting]) -> Option<&'a Meeting> {
    let kind = if lower.contains("lab") {
        "LAB"
    } else if ["class time", "class period", "regular class", "lecture", "in class"].iter().any(|k| lower.contains(k)) {
        "LEC"
    } else {
        return None;
    };
    meetings.iter().find(|m| m.kind.starts_with(kind)).or_else(|| (kind == "LEC").then(|| meetings.first()).flatten())
}

/// Split on ';' and on sentence ends (". " followed by a capital), but not "a.m. to".
fn clauses(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for part in line.split(';') {
        let b = part.as_bytes();
        let mut start = 0;
        for i in 0..b.len().saturating_sub(2) {
            if b[i] == b'.' && b[i + 1] == b' ' && b[i + 2].is_ascii_uppercase() {
                out.push(&part[start..i]);
                start = i + 2;
            }
        }
        out.push(&part[start..]);
    }
    out
}

fn parse_clause(clause: &str, year: i32, meetings: &[Meeting]) -> Vec<FoundExam> {
    let lower = clause.to_lowercase();
    if SKIP.iter().any(|s| lower.contains(s)) {
        return vec![];
    }
    let Some(kw) = KEYWORD.captures(clause) else { return vec![] };
    let word = kw.get(2).unwrap().as_str().to_lowercase();
    // A bare "final" must read like an event ("Final on …", "Final: …").
    if word == "final" {
        let after = lower[kw.get(0).unwrap().end()..].trim_start();
        if !(after.starts_with("on ") || after.starts_with(':') || after.starts_with("will")) {
            return vec![];
        }
    }

    // "Exam 1: Tuesday, 9/29 …" — the text before the colon names the exam.
    let (label, rest) = match clause.split_once(':') {
        Some((head, tail)) if KEYWORD.is_match(head) && head.split_whitespace().count() <= 6 && DATE.is_match(tail) => {
            (head.trim().to_string(), tail)
        }
        _ => (label_from_keyword(&kw), clause),
    };

    let meeting = session(&lower, meetings);
    // (date, is "week of …")
    let dates: Vec<(NaiveDate, bool)> = DATE
        .captures_iter(rest)
        .filter_map(|c| {
            let week = WEEK_OF.is_match(&rest[..c.get(0).unwrap().start()]);
            Some((to_date(&c, year)?, week))
        })
        .collect();
    if dates.is_empty() || dates.len() > 3 {
        return vec![];
    }
    let explicit_time = TIME.captures(rest).and_then(|c| to_time(&c[1], c.get(2).map(|m| m.as_str()), &c[3]));
    let time = explicit_time.or(meeting.and_then(|m| m.start));
    let plural = word.ends_with('s') && dates.len() > 1;
    dates
        .iter()
        .enumerate()
        .map(|(i, &(d, week))| {
            // "Week of Sep 28" + "during your lab" -> that week's lab day.
            let pinned = week
                .then(|| {
                    meeting.and_then(|m| (0..7).map(|k| d + Duration::days(k)).find(|x| m.days.contains(&x.weekday())))
                })
                .flatten();
            let approx = week && pinned.is_none();
            FoundExam {
                label: clean_label(&if plural { format!("{} {}", singular(&label), i + 1) } else { label.clone() }),
                date: pinned.unwrap_or(d),
                time: if approx || dates.len() > 1 && explicit_time.is_some() { None } else { time },
                approx,
            }
        })
        .collect()
}

/// "FINAL EXAM – CUMULATIVE" -> "Final Exam"; "Exam #3 - Requires …" -> "Exam #3".
pub fn clean_label(label: &str) -> String {
    let cut = [" - ", " – ", " — ", " [", " ("].iter().filter_map(|sep| label.find(sep)).min();
    let s = match cut {
        Some(i) if i >= 4 => label[..i].trim(),
        _ => label.trim(),
    };
    let shouting = s.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
    if !shouting {
        return s.to_string();
    }
    s.split_whitespace()
        .map(|w| {
            let l = w.to_lowercase();
            let mut c = l.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn label_from_keyword(kw: &regex::Captures) -> String {
    let ord = kw.get(1).map(|m| m.as_str());
    let word = kw.get(2).unwrap().as_str();
    let num = kw.get(3).map(|m| m.as_str());
    let mut s = String::new();
    if let Some(o) = ord {
        s.push_str(&capitalize(o));
        s.push(' ');
        s.push_str(&capitalize(&word.to_lowercase()));
    } else {
        s.push_str(&capitalize(&word.to_lowercase()));
    }
    if let Some(n) = num {
        s.push(' ');
        s.push_str(n);
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn singular(label: &str) -> String {
    let l = label.trim_end_matches('s');
    // "Midterm exam" -> "Midterm" reads better with a number after it.
    l.strip_suffix(" exam").unwrap_or(l).to_string()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn to_date(c: &regex::Captures, year: i32) -> Option<NaiveDate> {
    let y = |m: Option<regex::Match>| -> i32 {
        m.and_then(|m| m.as_str().parse::<i32>().ok()).map(|y| if y < 100 { 2000 + y } else { y }).unwrap_or(year)
    };
    if let (Some(m), Some(d)) = (c.get(1), c.get(2)) {
        return NaiveDate::from_ymd_opt(y(c.get(3)), m.as_str().parse().ok()?, d.as_str().parse().ok()?);
    }
    let mon = c.get(4)?.as_str().to_lowercase();
    let month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter()
        .position(|m| mon.starts_with(m))? as u32
        + 1;
    NaiveDate::from_ymd_opt(y(c.get(6)), month, c.get(5)?.as_str().parse().ok()?)
}

fn to_time(h: &str, m: Option<&str>, ap: &str) -> Option<NaiveTime> {
    let mut h: u32 = h.parse().ok()?;
    let m: u32 = m.and_then(|m| m.parse().ok()).unwrap_or(0);
    let pm = ap.eq_ignore_ascii_case("p");
    if pm && h < 12 {
        h += 12;
    }
    if !pm && h == 12 {
        h = 0;
    }
    NaiveTime::from_hms_opt(h, m, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }
    fn t(h: u32, m: u32) -> Option<NaiveTime> {
        NaiveTime::from_hms_opt(h, m, 0)
    }

    #[test]
    fn colon_labels_with_class_time() {
        let s = "Start Time: 11:10AM\nExam 1: Tuesday, 9/29 in our classroom during our class time (for 60 minutes)\n\
                 Final Exam: Thursday, 12/10, starting at 1:00 PM in our classroom (for 90 minutes)";
        let e = from_syllabus(s, 2026);
        assert_eq!(e[0], FoundExam { label: "Exam 1".into(), date: d(9, 29), time: t(11, 10), approx: false });
        assert_eq!(e[1], FoundExam { label: "Final Exam".into(), date: d(12, 10), time: t(13, 0), approx: false });
    }

    #[test]
    fn prose_with_several_dates() {
        let s = "The midterm exams will take place on Mon, Sep 21, and Mon, Oct 26, during the regular class time and in the usual classroom. Each exam will be 40 minutes long.\n\
                 The final exam will take place on Tue, Dec 8, from 10:00 a.m. to 12:00 p.m. and will cover all material.\n\
                 Week 16: No class on Mon, Dec 7; Final on Tue, Dec 8";
        let e = from_syllabus(s, 2026);
        assert_eq!(e.len(), 3);
        assert_eq!((e[0].label.as_str(), e[0].date), ("Midterm 1", d(9, 21)));
        assert_eq!((e[1].label.as_str(), e[1].date), ("Midterm 2", d(10, 26)));
        assert_eq!(e[2], FoundExam { label: "Final exam".into(), date: d(12, 8), time: t(10, 0), approx: false });
    }

    /// A Simple Syllabus "Important Dates" block as rendered (NBSPs included).
    #[test]
    fn stacked_weeks_pinned_to_lab_day() {
        let s = "Meeting Times:\u{a0}Meeting Type: LEC\nMeeting Days: TR\nStart Time: 11:10AM\nEnd Time: 12:25PM\n\
                 Meeting Times:\u{a0}Meeting Type: LAB\nMeeting Days: W\nStart Time: 3:00PM\nEnd Time: 3:50PM\n\n\
                 Important Dates\nExam Dates\nExam 1\n\nWeek of Sep 28\n\nTake the exam during your lab session (M or W).\n\n\
                 Exam 2\n\nWeek of Oct 26\n\nTake the exam during your lab session (M or W).\n\n\
                 Final\u{a0}Exam\n\nMonday, December 7 from 3:00 p.m. - 5:00 p.m. in\u{a0}ENGR\u{a0}101\n\n\
                 Labwork Due Dates\n\nLabs are done during your lab session.\n\n\
                 Exam 1 Make Up: Monday, October 5 at 7:30 PM";
        let e = from_syllabus(s, 2026);
        assert_eq!(e.len(), 3, "{e:?}");
        assert_eq!(e[0], FoundExam { label: "Exam 1".into(), date: d(9, 30), time: t(15, 0), approx: false });
        assert_eq!(e[1], FoundExam { label: "Exam 2".into(), date: d(10, 28), time: t(15, 0), approx: false });
        assert_eq!(e[2], FoundExam { label: "Final Exam".into(), date: d(12, 7), time: t(15, 0), approx: false });
    }

    #[test]
    fn week_without_a_meeting_stays_approximate() {
        let e = from_syllabus("Midterm\n\nWeek of Oct 12\n", 2026);
        assert_eq!(e, vec![FoundExam { label: "Midterm".into(), date: d(10, 12), time: None, approx: true }]);
    }

    #[test]
    fn ignores_tables_reviews_and_makeups() {
        let s = "6\t9/29, 10/1\tExam 1; Proof by Induction\n\
                 Exam 1 Make Up: Monday, October 5 at 7:30 PM\n\
                 Week 9: Midterm 2 review on Oct 20\n\
                 An exercise sheet will be posted every Wednesday and due the following Wednesday at 6:00 p.m.\n\
                 Exams 45%";
        assert!(from_syllabus(s, 2026).is_empty());
    }

    #[test]
    fn headings_are_not_labels() {
        let s = "Start Time: 9:10AM\nMidterm and Final Exams\n\n\
                 The midterm exams will take place on Mon, Sep 21, and Mon, Oct 26, during the regular class time and in the usual classroom.";
        let e = from_syllabus(s, 2026);
        assert_eq!(e.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), ["Midterm 1", "Midterm 2"]);
        assert_eq!(e[0].time, t(9, 10));
    }

    #[test]
    fn labels_are_cleaned() {
        assert_eq!(clean_label("FINAL EXAM – CUMULATIVE"), "Final Exam");
        assert_eq!(clean_label("Exam #3 - Requires Respondus LockDown Browser + Webcam"), "Exam #3");
        assert_eq!(clean_label("Exam 1 [Tue 9/29]"), "Exam 1");
    }

    #[test]
    fn titles() {
        assert!(is_exam_title("Exam #2 - Requires Respondus LockDown Browser + Webcam"));
        assert!(is_exam_title("First Midterm"));
        assert!(is_exam_title("Final Exam - Cumulative"));
        assert!(!is_exam_title("Weighted Midterm Exam Average"));
        assert!(!is_exam_title("Final Project Report"));
        assert!(!is_exam_title("Midterm 2 review"));
        assert!(!is_exam_title("Sheet 9"));
    }
}
