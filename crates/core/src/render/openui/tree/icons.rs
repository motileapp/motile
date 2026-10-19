//! Lucide icons by the names programs write (`glyph.tsx`).

use super::Glyph;
use super::lucide::NAMES;

/// The icon a name means: "MapPin", "map_pin" and "map pin" all mean "map-pin", and an alias the
/// icon it stands for. None for a name Lucide doesn't have.
pub fn glyph(name: &str) -> Option<Glyph> {
    let name = kebab(name.trim());
    let index = NAMES.binary_search_by(|(key, _, _)| (*key).cmp(name.as_str())).ok()?;
    let (_, icon, character) = NAMES[index];
    Some(Glyph { name: icon.to_owned(), glyph: character.to_string() })
}

/// `([a-z0-9])([A-Z])` to `$1-$2`, runs of white space and `_` to `-`, then lower case.
fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut previous: Option<char> = None;
    let mut in_gap = false;
    for character in name.chars() {
        if character.is_whitespace() || character == '_' {
            if !in_gap {
                out.push('-');
            }
            in_gap = true;
            previous = Some(character);
            continue;
        }
        in_gap = false;
        if character.is_ascii_uppercase()
            && previous.is_some_and(|before| before.is_ascii_lowercase() || before.is_ascii_digit())
        {
            out.push('-');
        }
        out.push(character);
        previous = Some(character);
    }
    out.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(name: &str) -> Option<(String, u32)> {
        glyph(name).map(|glyph| (glyph.name, glyph.glyph.chars().next().unwrap() as u32))
    }

    #[test]
    fn reads_names_as_lucide_react_does() {
        assert_eq!(found("map-pin"), Some(("map-pin".into(), 0xe111)));
        assert_eq!(found("MapPin"), found("map-pin"));
        assert_eq!(found("map_pin"), found("map-pin"));
        assert_eq!(found(" map pin "), found("map-pin"));
        assert_eq!(found("arrowUpRight"), Some(("arrow-up-right".into(), 0xe04d)));
    }

    #[test]
    fn takes_aliases_for_the_icon_they_stand_for() {
        assert_eq!(found("home"), Some(("house".into(), 0xe0f5)));
        assert_eq!(found("check-circle-2"), Some(("circle-check".into(), 0xe226)));
    }

    #[test]
    fn knows_nothing_of_unknown_names_or_icons_the_font_lacks() {
        assert_eq!(found("no-such-icon"), None);
        assert_eq!(found("github"), None);
        assert_eq!(found(""), None);
    }
}
