//! Helpers shared by the platform log writers: `android::logcat` and
//! `apple::oslog`.
//!
//! Both platforms cap the size of one log entry, and both split a longer
//! message across consecutive entries rather than lose its tail. The cap
//! differs, so it stays with each writer; the splitting lives here so the two
//! cannot drift.

/// Splits `text` into pieces of at most `max` bytes, each ending on a
/// character boundary.
pub(crate) fn chunks(text: &str, max: usize) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut end = rest.len().min(max);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        // Only reachable if `max` is smaller than the first character, which
        // every caller's constant rules out; taking that one character
        // regardless keeps a future caller from spinning forever on an empty
        // chunk.
        if end == 0 {
            end = rest.chars().next().map_or(rest.len(), char::len_utf8);
        }
        let (head, tail) = rest.split_at(end);
        rest = tail;
        Some(head)
    })
}

#[cfg(test)]
mod tests {
    use super::chunks;

    #[test]
    fn chunks_respect_the_limit_and_character_boundaries() {
        // 'é' is two bytes, so a 3-byte limit has to stop before splitting one.
        let pieces: Vec<&str> = chunks("aéé", 3).collect();
        assert_eq!(pieces, ["aé", "é"]);
        assert!(pieces.iter().all(|piece| piece.len() <= 3));
    }

    #[test]
    fn chunks_of_nothing_is_nothing() {
        assert_eq!(chunks("", 10).count(), 0);
    }

    #[test]
    fn a_limit_smaller_than_a_character_still_makes_progress() {
        // '€' is three bytes; with a limit of one it must still be emitted
        // whole rather than looping on an empty chunk.
        let pieces: Vec<&str> = chunks("€€", 1).collect();
        assert_eq!(pieces, ["€", "€"]);
    }
}
