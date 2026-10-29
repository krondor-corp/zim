//! A small subsequence scorer for the command palette, free of the DOM so
//! it tests natively.
//!
//! A query matches a candidate when its characters appear in order
//! (case-insensitively). Matches score higher the tighter and earlier they
//! sit: consecutive characters and characters that start a word (after a
//! `/`, `-`, `_`, `.` or space) earn bonuses, so `nt` finds `notes/todo.md`
//! through its word starts before it finds `internet.md`.

/// Score `candidate` against `query`; `None` when it doesn't match.
/// Higher is better. An empty query matches everything with score 0.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let mut wanted = query.chars().flat_map(char::to_lowercase).peekable();
    let mut total = 0;
    let mut prev_matched = false;
    let mut prev: Option<char> = None;
    for (i, c) in candidate.chars().enumerate() {
        let Some(&w) = wanted.peek() else { break };
        if c.to_lowercase().next() == Some(w) {
            wanted.next();
            total += 1;
            if prev_matched {
                total += 5;
            }
            if prev.is_none_or(|p| matches!(p, '/' | '-' | '_' | '.' | ' ')) {
                total += 8;
            }
            if i == 0 {
                total += 4;
            }
            prev_matched = true;
        } else {
            prev_matched = false;
        }
        prev = Some(c);
    }
    wanted.peek().is_none().then(|| {
        // Between equally good matches, the shorter candidate wins.
        total * 100 - candidate.chars().count() as i32
    })
}

/// Indexes of the matching `candidates`, best first; ties keep their
/// input order. An empty query keeps every candidate in input order.
pub fn rank<S: AsRef<str>>(query: &str, candidates: &[S]) -> Vec<usize> {
    let mut hits: Vec<(usize, i32)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| score(query, c.as_ref()).map(|s| (i, s)))
        .collect();
    if !query.is_empty() {
        hits.sort_by_key(|&(i, s)| (std::cmp::Reverse(s), i));
    }
    hits.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_types_nothing_and_sees_everything_in_order() {
        let files = ["b.md", "a.md", "c.md"];
        assert_eq!(rank("", &files), vec![0, 1, 2]);
    }

    #[test]
    fn alice_skips_letters_but_not_their_order() {
        assert!(score("tdo", "notes/todo.md").is_some());
        assert!(score("odt", "notes/todo.md").is_none());
        assert!(score("zzz", "notes/todo.md").is_none());
    }

    #[test]
    fn alice_types_in_lowercase_and_still_finds_a_capitalised_file() {
        assert!(score("readme", "docs/README.md").is_some());
        assert!(score("README", "docs/readme.md").is_some());
    }

    #[test]
    fn alice_looks_for_todo_and_gets_the_exact_file_first() {
        let files = ["archive/old-notes.md", "todo.md", "project/to-do-later.md"];
        assert_eq!(rank("todo", &files)[0], 1);
    }

    #[test]
    fn alice_types_word_starts_and_lands_on_the_word_boundaries() {
        let files = ["internet.md", "notes/todo.md"];
        assert_eq!(rank("nt", &files)[0], 1);
    }

    #[test]
    fn alice_prefers_the_shorter_of_two_equal_matches() {
        let files = ["todo-list.md", "todo.md"];
        assert_eq!(rank("todo", &files)[0], 1);
    }

    #[test]
    fn alice_gets_only_what_matches() {
        let files = ["a.md", "b.md", "ab.md"];
        assert_eq!(rank("ab", &files), vec![2]);
    }

    #[test]
    fn bob_ties_keep_the_order_the_vault_listed_them() {
        let files = ["x/a.md", "y/a.md"];
        assert_eq!(rank("a.md", &files), vec![0, 1]);
    }
}
