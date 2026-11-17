//! Explicit skill references shared by every prompt surface.

pub(crate) fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// Ordered, deduplicated `$name` references. A bare dollar, a dollar amount,
/// or a dollar escaped with an odd number of backslashes stays literal.
/// The original prompt is never rewritten.
pub(crate) fn requested_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (start, _) in text.match_indices('$') {
        if text[..start]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count()
            % 2
            == 1
        {
            continue;
        }
        let tail = &text[start + 1..];
        let len = tail
            .chars()
            .take_while(|c| name_char(*c))
            .map(char::len_utf8)
            .sum::<usize>();
        let name = &tail[..len];
        if name.is_empty() || name.chars().all(|c| c.is_ascii_digit() || c == '.') {
            continue;
        }
        if !names.iter().any(|entry| entry == name) {
            names.push(name.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_references_are_ordered_unique_and_literal_dollars_stay_literal() {
        assert_eq!(
            requested_names("先$review then\n$test and $review"),
            ["review", "test"]
        );
        assert_eq!(
            requested_names(r"cost $20.50 and $ and \$HOME then $review"),
            ["review"]
        );
    }
}
