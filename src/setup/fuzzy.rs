//! The matcher behind the module picker's search (SPEC § 14): fuzzy and
//! initialism, so `sy` finds `sync` and `sn` finds `session_name`.

/// How well `candidate` matches `query`: higher is better, `None` is no
/// match. An empty query matches everything equally.
///
/// A substring match outranks an initialism (`sn` for `session_name`),
/// which outranks a subsequence (`ssn` for `session_name`), and among
/// substring matches an earlier one ranks higher. Case is ignored.
#[must_use]
pub fn score(query: &str, candidate: &str) -> Option<u32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let c = candidate.to_lowercase();
    if let Some(at) = c.find(&q.iter().collect::<String>()) {
        return Some(300_u32.saturating_sub(u32::try_from(at).unwrap_or(u32::MAX).min(100)));
    }
    let initials: Vec<char> =
        c.split(['_', '-', '.', ' ']).filter_map(|word| word.chars().next()).collect();
    if initials.starts_with(&q) {
        return Some(200);
    }
    // A subsequence: every query character in order, scored by how tightly
    // they sit together.
    let chars: Vec<char> = c.chars().collect();
    let mut at = 0_usize;
    let mut gaps = 0_usize;
    for ch in &q {
        let found = chars.get(at..)?.iter().position(|x| x == ch)?;
        gaps = gaps.saturating_add(found);
        at = at.saturating_add(found).saturating_add(1);
    }
    Some(100_u32.saturating_sub(u32::try_from(gaps).unwrap_or(u32::MAX).min(99)))
}

/// The places of the candidates that match `query`, best first, ties in
/// the order given: places, so two candidates spelled alike stay two.
#[must_use]
pub fn rank<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let mut scored: Vec<(u32, usize)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| score(query, c).map(|s| (s, i)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: [&str; 6] = ["session", "session_name", "sync", "style", "spend", "text.motd"];

    fn names(query: &str) -> Vec<&'static str> {
        rank(query, IDS).into_iter().map(|i| IDS[i]).collect()
    }

    #[test]
    fn substring_then_initialism_then_subsequence() {
        assert_eq!(names("sy").first(), Some(&"sync"));
        assert_eq!(names("sn").first(), Some(&"session_name"));
        assert_eq!(names("name"), vec!["session_name"]);
        assert_eq!(names("tm").first(), Some(&"text.motd"), "a dot separates words");
        assert_eq!(names("zz"), Vec::<&str>::new());
        assert_eq!(names(""), IDS.to_vec(), "an empty query keeps the order");
        assert_eq!(rank("a", ["a", "b", "a"]), vec![0, 2], "alike stays two");
        assert!(score("SES", "session") > score("ssn", "session"));
        assert!(score("ssn", "session_name").is_some());
        assert!(score("sess", "session").unwrap() > score("sess", "my_session").unwrap());
    }
}
