//! What finding in the thread found in a row, set off where its text is drawn. The core says
//! where each match is in the row's text in UTF-16 units; a row draws its text in stretches of
//! their own, each told by where it starts in the row's.

use std::ops::Range;

use motile_core::find::FoundRow;

/// The matches in one row, and which of them is the current one.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Marks {
    pub ranges: Vec<[u32; 2]>,
    pub current: Option<usize>,
}

impl From<FoundRow> for Marks {
    fn from(row: FoundRow) -> Self {
        Self { ranges: row.ranges, current: row.current }
    }
}

impl Marks {
    /// The matches in `text`, which starts `source` units into the row's text, as bytes of it,
    /// each saying whether it is the current match.
    pub fn in_text(&self, text: &str, source: u32) -> Vec<(Range<usize>, bool)> {
        let mut bytes = Vec::with_capacity(text.len() + 1);
        for (at, character) in text.char_indices() {
            bytes.extend(std::iter::repeat_n(at, character.len_utf16()));
        }
        bytes.push(text.len());
        let count = bytes.len() as u32 - 1;
        self.ranges
            .iter()
            .enumerate()
            .filter_map(|(index, [start, len])| {
                let from = start.saturating_sub(source).min(count);
                let to = (start + len).saturating_sub(source).min(count);
                (to > from).then(|| (bytes[from as usize]..bytes[to as usize], self.current == Some(index)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_is_bytes_of_the_stretch_it_falls_in() {
        let marks = Marks { ranges: vec![[2, 4], [9, 3]], current: Some(1) };
        assert_eq!(marks.in_text("café au lait", 0), vec![(2..7, false), (10..13, true)]);
        assert_eq!(marks.in_text("lait", 8), vec![(1..4, true)], "a stretch further on");
        assert!(marks.in_text("ca", 0).iter().all(|(range, _)| range.end <= 2), "cut at the stretch's end");
    }
}
