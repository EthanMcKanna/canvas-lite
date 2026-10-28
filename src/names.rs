//! Turns registrar-style course names into something readable.
//!
//!   "26 FALL COMP 120 (250,500-511): PROGRAM DESIGN & CONCEPTS"
//!     -> label "COMP 120", title "Program Design & Concepts", term "Fall 2026"

const TERM_WORDS: &[&str] =
    &["FALL", "SPRING", "SUMMER", "WINTER", "SPR", "SUM", "FA", "SP", "SU", "WI", "TERM", "YEAR"];
const SMALL_WORDS: &[&str] = &["a", "an", "and", "as", "at", "by", "for", "in", "of", "on", "or", "the", "to", "with"];

/// A subject + number match, e.g. "COMP 120", and the byte range it covered.
fn find_code(s: &str) -> Option<(String, usize, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Subject: 2–5 letters at a word boundary.
        if b[i].is_ascii_alphabetic() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let start = i;
            let mut j = i;
            while j < b.len() && b[j].is_ascii_alphabetic() {
                j += 1;
            }
            let subj = &s[start..j];
            let ok_subj = (2..=5).contains(&subj.len())
                && subj.bytes().all(|c| c.is_ascii_uppercase())
                && !TERM_WORDS.contains(&subj);
            // Optional single separator, then 3–4 digits (+ optional letter suffix like 101L).
            let mut k = j;
            if k < b.len() && matches!(b[k], b' ' | b'-' | b'_') {
                k += 1;
            }
            let num_start = k;
            while k < b.len() && b[k].is_ascii_digit() {
                k += 1;
            }
            let digits = k - num_start;
            if k < b.len() && b[k].is_ascii_uppercase() && (k + 1 == b.len() || !b[k + 1].is_ascii_alphanumeric()) {
                k += 1;
            }
            let bounded = k == b.len() || !b[k].is_ascii_alphanumeric();
            if ok_subj && (3..=4).contains(&digits) && bounded {
                return Some((format!("{subj} {}", &s[num_start..k]), start, k));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

/// "COMP 120" from the course name, or from the course code when that code is
/// clean (only section numbers after it, not something like "COMU-173-CAAG").
pub fn short_code(name: &str, code: Option<&str>) -> Option<String> {
    if let Some((c, ..)) = find_code(name) {
        return Some(c);
    }
    let code = code?;
    let (c, _, end) = find_code(code)?;
    code[end..].bytes().all(|ch| !ch.is_ascii_alphabetic()).then_some(c)
}

/// Human title: the part after the code/colon, title-cased if it was SHOUTING.
pub fn title(name: &str) -> String {
    let rest = match name.rfind(':') {
        Some(i) if !name[i + 1..].trim().is_empty() => &name[i + 1..],
        _ => match find_code(name) {
            // "26 FALL COMP 120 Program Design" -> "Program Design"
            Some((_, _, end)) if !name[end..].trim().is_empty() => &name[end..],
            _ => name,
        },
    };
    let rest = rest.trim().trim_start_matches(['-', '–', '(']).trim();
    let shouting = rest.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
    if shouting { title_case(rest) } else { rest.to_string() }
}

/// The best short label for tight spaces: the code, or failing that a trimmed name.
pub fn label(name: &str, code: Option<&str>) -> String {
    short_code(name, code).unwrap_or_else(|| {
        let t = title(name);
        // Drop campus/location suffixes: "COMP Advising - Main Campus" -> "COMP Advising"
        t.split(" - ").next().unwrap_or(&t).trim().to_string()
    })
}

/// Drop due-date annotations that repeat what the UI already shows:
/// "Submit hw6.pdf [Due Thu 10/29]" -> "Submit hw6.pdf", "Exam 1 [Tue 9/29]" -> "Exam 1".
pub fn clean_title(t: &str) -> String {
    static DUE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)\s*[\[(]\s*(?:due\b[^\])]*|(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*\.?,?\s+\d{1,2}/\d{1,2}[^\])]*)[\])]",
        )
        .unwrap()
    });
    static JOIN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(\S)\s*[-–]\s+").unwrap());
    let s = DUE.replace_all(t, "");
    // What was "…10/26]- Requires" becomes "… – Requires".
    let s = JOIN.replace_all(&s, "$1 – ");
    let s = s.trim().trim_end_matches(['-', '–', ':']).trim();
    if s.is_empty() { t.trim().to_string() } else { s.to_string() }
}

/// "Fall 2026" from names like "26 FALL …" or "FALL 2026 …".
pub fn term(name: &str) -> Option<String> {
    let words: Vec<&str> = name.split_whitespace().take(4).collect();
    for (i, w) in words.iter().enumerate() {
        let season = match w.to_ascii_uppercase().as_str() {
            "FALL" | "FA" => "Fall",
            "SPRING" | "SPR" | "SP" => "Spring",
            "SUMMER" | "SUM" | "SU" => "Summer",
            "WINTER" | "WI" => "Winter",
            _ => continue,
        };
        let year = [i.checked_sub(1), Some(i + 1)]
            .into_iter()
            .flatten()
            .filter_map(|j| words.get(j))
            .find_map(|y| y.parse::<u32>().ok().filter(|y| *y < 100 || (2000..2100).contains(y)))?;
        let year = if year < 100 { 2000 + year } else { year };
        return Some(format!("{season} {year}"));
    }
    None
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .enumerate()
        .map(|(i, w)| {
            let lower = w.to_lowercase();
            let roman = !w.is_empty() && w.chars().all(|c| matches!(c, 'I' | 'V' | 'X'));
            if roman {
                w.to_string()
            } else if i > 0 && SMALL_WORDS.contains(&lower.as_str()) {
                lower
            } else {
                let mut c = lower.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrar_style() {
        let n = "26 FALL COMP 120 (250,500-511,518-526,550): PROGRAM DESIGN & CONCEPTS";
        assert_eq!(label(n, Some("COMP-120:250,500,501")), "COMP 120");
        assert_eq!(title(n), "Program Design & Concepts");
        assert_eq!(term(n).as_deref(), Some("Fall 2026"));
        assert_eq!(label("26 FALL BIOL 201 700: PLANT SCIENCE & PRACTICES", None), "BIOL 201");
        assert_eq!(title("26 FALL COMP 181 599: INTRO TO COMPUTING"), "Intro to Computing");
    }

    #[test]
    fn titles_lose_due_annotations() {
        assert_eq!(clean_title("Submit your hw6.pdf file here [Due Thu 10/29]"), "Submit your hw6.pdf file here");
        assert_eq!(clean_title("Exercise 14 [due Fri 11/6]"), "Exercise 14");
        assert_eq!(
            clean_title("Quiz 6 [Due Mon 10/26]- Requires Respondus LockDown Browser"),
            "Quiz 6 – Requires Respondus LockDown Browser"
        );
        assert_eq!(clean_title("Exam 1 [Tue 9/29]"), "Exam 1");
        assert_eq!(clean_title("[Section 2] Class Activity 10.1"), "[Section 2] Class Activity 10.1");
    }

    #[test]
    fn non_course_falls_back_to_name() {
        let n = "COMP Undergraduate Advising - Main Campus";
        assert_eq!(label(n, Some("ADVS-173-CAAG")), "COMP Undergraduate Advising");
        assert_eq!(title(n), n);
    }

    #[test]
    fn other_schools() {
        assert_eq!(label("Fall 2026 - CS 3110: Functional Programming", None), "CS 3110");
        assert_eq!(term("Fall 2026 - CS 3110").as_deref(), Some("Fall 2026"));
        assert_eq!(label("Organic Chemistry", Some("CHEM_2410")), "CHEM 2410");
        assert_eq!(label("Bio Lab", Some("BIOL 101L-02")), "BIOL 101L");
        assert_eq!(title("CALCULUS II"), "Calculus II");
    }
}
