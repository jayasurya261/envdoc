/// Computes the Levenshtein edit distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];

    for (i, &a_ch) in a_chars.iter().enumerate() {
        curr_row[0] = i + 1;
        for (j, &b_ch) in b_chars.iter().enumerate() {
            let cost = if a_ch == b_ch { 0 } else { 1 };
            curr_row[j + 1] = std::cmp::min(
                std::cmp::min(curr_row[j] + 1, prev_row[j + 1] + 1),
                prev_row[j] + cost,
            );
        }
        prev_row.clone_from_slice(&curr_row);
    }

    prev_row[b_len]
}

/// Finds the closest matching contract name for a suspected typo.
///
/// Returns `Some(suggestion)` if a candidate has an edit distance <= 2,
/// or distance / len <= 0.35.
pub fn find_typo_suggestion<'a>(
    unknown: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let mut best_match: Option<(&'a str, usize)> = None;
    let unk_len = unknown.len();

    for candidate in candidates {
        let dist = levenshtein_distance(unknown, candidate);
        let max_len = std::cmp::max(unk_len, candidate.len());

        let is_candidate = if max_len <= 5 {
            dist <= 1
        } else if max_len <= 10 {
            dist <= 2
        } else {
            dist <= 3 && (dist as f32 / max_len as f32) <= 0.35
        };

        if is_candidate {
            match best_match {
                None => best_match = Some((candidate, dist)),
                Some((_, best_dist)) if dist < best_dist => best_match = Some((candidate, dist)),
                _ => {}
            }
        }
    }

    best_match.map(|(name, _)| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("DATABSE_URL", "DATABASE_URL"), 1);
        assert_eq!(levenshtein_distance("PORT", "PORT"), 0);
        assert_eq!(levenshtein_distance("API_KEY", "APP_KEY"), 1);
        assert_eq!(levenshtein_distance("HOST", "POST"), 1);
    }

    #[test]
    fn test_find_typo_suggestion() {
        let candidates = ["DATABASE_URL", "PORT", "API_KEY", "REDIS_HOST"];
        assert_eq!(
            find_typo_suggestion("DATABSE_URL", candidates.iter().copied()),
            Some("DATABASE_URL".to_string())
        );
        assert_eq!(
            find_typo_suggestion("COMPLETELY_DIFFERENT", candidates.iter().copied()),
            None
        );
    }
}
