//! The skills the composer offers while a word starts with `$` or `/`: those of the composer's
//! agent's account in its project, best match first, and the chips a message's `$name`s become.
//! Every client shows the same.

use motile_protocol::skills::is_name_character;
use motile_protocol::wire::{Skill, SkillMention, SkillSource};
use serde::Serialize;

use crate::render::rows::{SkillChip, skill_chips};

/// What the composer shows while a skill is typed: the word typed so far, which the chosen
/// skill replaces, and the skills it could be. No rows when nothing fits.
#[derive(Serialize, Debug, PartialEq, Default)]
pub struct Menu {
    pub typed: String,
    pub rows: Vec<MenuRow>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct MenuRow {
    pub name: String,
    pub label: String,
    pub description: String,
    /// Where it comes from, and the symbol that says so.
    pub source: &'static str,
    pub symbol: &'static str,
}

/// The word that ends `before_cursor` when it starts with `$` or `/` and could go on to be a
/// skill's name.
pub fn typed(before_cursor: &str) -> Option<&str> {
    let start = before_cursor.rfind(char::is_whitespace).map_or(0, |at| at + 1);
    let word = &before_cursor[start..];
    let query = word.strip_prefix(['$', '/'])?;
    query.chars().all(is_name_character).then_some(word)
}

/// The menu for `before_cursor`, or `None` when no skill fits what is typed.
pub fn menu(before_cursor: &str, skills: &[Skill]) -> Option<Menu> {
    let typed = typed(before_cursor)?;
    let query = typed[1..].to_lowercase();
    let mut ranked: Vec<(u32, &Skill)> =
        skills.iter().filter_map(|skill| Some((score(skill, &query)?, skill))).collect();
    ranked.sort_by_key(|(score, skill)| (*score, skill.label.to_lowercase()));
    let rows: Vec<MenuRow> = ranked
        .into_iter()
        .map(|(_, skill)| MenuRow {
            name: skill.name.clone(),
            label: skill.label.clone(),
            description: skill.description.clone(),
            source: source_label(skill.source),
            symbol: source_symbol(skill.source),
        })
        .collect();
    (!rows.is_empty()).then(|| Menu { typed: typed.to_string(), rows })
}

/// How well `skill` fits `query`, lower first: its name or label as typed, starting with it, a
/// word of it starting with it, holding it, or holding its letters in order; then its
/// description holding it. Nothing typed lists the project's first, then the user's.
fn score(skill: &Skill, query: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(source_order(skill.source));
    }
    let named = [&skill.name, &skill.label].into_iter().filter_map(|field| fit(&field.to_lowercase(), query)).min();
    let described = || skill.description.to_lowercase().contains(query).then_some(10);
    named.or_else(described)
}

pub(crate) fn fit(field: &str, query: &str) -> Option<u32> {
    let starts_word =
        || field.split(|character: char| !character.is_alphanumeric()).any(|word| word.starts_with(query));
    let in_order = || {
        let mut letters = field.chars();
        query.chars().all(|wanted| letters.any(|character| character == wanted))
    };
    match () {
        _ if field == query => Some(0),
        _ if field.starts_with(query) => Some(1),
        _ if starts_word() => Some(2),
        _ if field.contains(query) => Some(3),
        _ if in_order() => Some(4),
        _ => None,
    }
}

fn source_order(source: SkillSource) -> u32 {
    match source {
        SkillSource::Project => 0,
        SkillSource::Personal => 1,
        SkillSource::Plugin => 2,
        SkillSource::System => 3,
    }
}

fn source_label(source: SkillSource) -> &'static str {
    match source {
        SkillSource::Project => "Project",
        SkillSource::Personal => "Personal",
        SkillSource::Plugin => "Plugin",
        SkillSource::System => "Built in",
    }
}

/// Lucide's name for the symbol.
fn source_symbol(source: SkillSource) -> &'static str {
    match source {
        SkillSource::Project => "folder",
        SkillSource::Personal => "user-round",
        SkillSource::Plugin => "blocks",
        SkillSource::System => "box",
    }
}

/// The `$name`s of `text` that are skills of `skills`, as chips.
pub fn chips(text: &str, skills: &[Skill]) -> Vec<SkillChip> {
    let mentions: Vec<SkillMention> =
        skills.iter().map(|skill| SkillMention { name: skill.name.clone(), label: skill.label.clone() }).collect();
    skill_chips(text, &mentions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str, label: &str, description: &str, source: SkillSource) -> Skill {
        Skill { name: name.into(), label: label.into(), description: description.into(), source }
    }

    fn skills() -> Vec<Skill> {
        vec![
            skill("imagegen", "Image Gen", "Generate or edit images", SkillSource::System),
            skill("review", "review", "Review the changes on this branch", SkillSource::Personal),
            skill("ship-it", "ship-it", "Commit, push and open a pull request", SkillSource::Project),
            skill("pdf", "pdf", "Read and fill PDF files", SkillSource::Plugin),
        ]
    }

    fn names(before_cursor: &str) -> Vec<String> {
        menu(before_cursor, &skills())
            .map(|menu| menu.rows.into_iter().map(|row| row.name).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_word_that_starts_with_a_dollar_or_a_slash_is_a_skill_being_typed() {
        assert_eq!(typed("use $sh"), Some("$sh"));
        assert_eq!(typed("/"), Some("/"));
        assert_eq!(typed("open /Users/me"), None, "a path");
        assert_eq!(typed("use $ship-it "), None, "the word has ended");
        assert_eq!(typed("a$b"), None);
    }

    #[test]
    fn nothing_typed_lists_the_project_s_skills_first_then_the_user_s() {
        assert_eq!(names("$"), ["ship-it", "review", "pdf", "imagegen"]);
    }

    #[test]
    fn the_skills_that_fit_best_come_first() {
        assert_eq!(names("/i"), ["imagegen", "ship-it", "review", "pdf"], "its name starts with it, then a word of it");
        assert_eq!(names("$gen"), ["imagegen"], "a word of its label starts with it");
        assert_eq!(names("$pull"), ["ship-it"], "its description has it");
        assert_eq!(names("$zz"), Vec::<String>::new());
        assert!(menu("$zz", &skills()).is_none(), "no menu when nothing fits");
    }

    #[test]
    fn a_menu_says_what_it_replaces_and_where_each_skill_comes_from() {
        let menu = menu("run $ship", &skills()).unwrap();
        assert_eq!(menu.typed, "$ship");
        assert_eq!((menu.rows[0].source, menu.rows[0].symbol), ("Project", "folder"));
    }
}
