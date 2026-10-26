//! Browsing a server's folders by typing a path, the way a shell completes one: the folders of
//! the directory typed so far, narrowed by what follows its last slash. And searching the images
//! in a project's folder for its icon. Every client shows the same.

use serde::Serialize;

use crate::skills::fit;

const MAX_FOUND: usize = 200;
/// What an image that is meant as an icon is usually called.
const ICON_WORDS: [&str; 3] = ["icon", "logo", "favicon"];

/// The path typed so far: the directory to list and the start of a folder's name in it.
#[derive(Debug, PartialEq)]
pub struct Typed {
    pub directory: String,
    pub leaf: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Listing {
    /// The directory that is listed, as an absolute path and as it is typed.
    pub path: String,
    pub typed: String,
    /// What to type for the directory above. Missing at the root.
    pub parent: Option<String>,
    pub folders: Vec<Folder>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Folder {
    pub name: String,
    pub path: String,
    /// What to type to look inside it.
    pub typed: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Image {
    pub name: String,
    /// The folder it is in, inside the project's. Empty at its top.
    pub folder: String,
    pub path: String,
}

/// `None` when the path doesn't start at the root or at the home folder.
pub fn typed(query: &str, home: &str) -> Option<Typed> {
    let home = home.trim_end_matches('/');
    let path = match query.strip_prefix('~') {
        Some("") => format!("{home}/"),
        Some(rest) if rest.starts_with('/') => format!("{home}{rest}"),
        _ => query.to_string(),
    };
    if !path.starts_with('/') {
        return None;
    }
    let (directory, leaf) = path.rsplit_once('/')?;
    let directory = directory.trim_end_matches('/');
    Some(Typed { directory: if directory.is_empty() { "/" } else { directory }.to_string(), leaf: leaf.to_string() })
}

/// How a directory is typed: from `~` when it is in the home folder, and ending in a slash.
fn as_typed(path: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    let path = path.trim_end_matches('/');
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}/"),
        _ => format!("{path}/"),
    }
}

/// The folders of `directory` whose names start with `leaf`, whatever the case.
pub fn listing(directory: &str, leaf: &str, folders: &[String], home: &str) -> Listing {
    let leaf = leaf.to_lowercase();
    let base = directory.trim_end_matches('/');
    let named = |name: &&String| name.to_lowercase().starts_with(&leaf);
    let folder = |name: &String| {
        let path = format!("{base}/{name}");
        Folder { name: name.clone(), typed: as_typed(&path, home), path }
    };
    let parent = (directory != "/").then(|| as_typed(base.rsplit_once('/').map_or("", |(parent, _)| parent), home));
    Listing {
        path: directory.to_string(),
        typed: as_typed(directory, home),
        parent,
        folders: folders.iter().filter(named).map(folder).collect(),
    }
}

/// The images at `paths` inside the project's `folder` that fit `query`, the best first: by their
/// name, then by their path. With nothing typed, those named like an icon come first. The least
/// deep come first among equals.
pub fn images(folder: &str, paths: &[String], query: &str) -> Vec<Image> {
    let query = query.trim().to_lowercase();
    let score = |path: &String| {
        let path = path.to_lowercase();
        let name = path.rsplit('/').next().unwrap_or_default();
        if query.is_empty() {
            return Some(if ICON_WORDS.iter().any(|word| name.contains(word)) { 0 } else { 1 });
        }
        fit(name, &query).or_else(|| fit(&path, &query).map(|score| score + 10))
    };
    let mut ranked: Vec<(u32, usize, &String)> =
        paths.iter().filter_map(|path| Some((score(path)?, path.matches('/').count(), path))).collect();
    ranked.sort();
    let base = folder.trim_end_matches('/');
    let image = |path: &String| {
        let (inside, name) = path.rsplit_once('/').unwrap_or(("", path));
        Image { name: name.to_string(), folder: inside.to_string(), path: format!("{base}/{path}") }
    };
    ranked.into_iter().take(MAX_FOUND).map(|(_, _, path)| image(path)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/ada";

    fn split(query: &str) -> Option<(String, String)> {
        typed(query, HOME).map(|typed| (typed.directory, typed.leaf))
    }

    #[test]
    fn a_typed_path_is_a_directory_and_the_start_of_a_name() {
        assert_eq!(split("~/"), Some((HOME.into(), "".into())));
        assert_eq!(split("~"), Some((HOME.into(), "".into())));
        assert_eq!(split("~/code/mo"), Some(("/home/ada/code".into(), "mo".into())));
        assert_eq!(split("/"), Some(("/".into(), "".into())));
        assert_eq!(split("/va"), Some(("/".into(), "va".into())));
        assert_eq!(split("/var/log/"), Some(("/var/log".into(), "".into())));
        assert_eq!(split("code/"), None);
        assert_eq!(split("~ada/"), None);
    }

    #[test]
    fn folders_are_narrowed_by_the_name_and_say_what_to_type() {
        let folders = ["Motile".to_string(), "notes".to_string(), "more".to_string()];
        let listed = listing("/home/ada/code", "mo", &folders, HOME);

        assert_eq!(listed.typed, "~/code/");
        assert_eq!(listed.parent.as_deref(), Some("~/"));
        let found: Vec<_> = listed.folders.iter().map(|folder| (folder.path.as_str(), folder.typed.as_str())).collect();
        assert_eq!(found, vec![("/home/ada/code/Motile", "~/code/Motile/"), ("/home/ada/code/more", "~/code/more/")]);
    }

    fn found(query: &str) -> Vec<String> {
        let paths = ["docs/shots/home.png", "apps/web/public/favicon.svg", "Logo.png", "src/assets/hero.webp"];
        let paths: Vec<String> = paths.map(str::to_string).to_vec();
        images("/home/ada/code/", &paths, query).into_iter().map(|image| image.path).collect()
    }

    #[test]
    fn images_named_like_an_icon_come_first() {
        let found = found("");
        assert_eq!(found[..2], ["/home/ada/code/Logo.png", "/home/ada/code/apps/web/public/favicon.svg"]);
    }

    #[test]
    fn images_are_found_by_their_name_then_by_their_path() {
        assert_eq!(found("logo"), ["/home/ada/code/Logo.png"]);
        assert_eq!(found("ho"), ["/home/ada/code/docs/shots/home.png", "/home/ada/code/src/assets/hero.webp"]);
        assert_eq!(found("public"), ["/home/ada/code/apps/web/public/favicon.svg"]);
        assert!(found("nothing like it").is_empty());

        let paths = ["apps/web/favicon.svg".to_string()];
        let image = &images("/home/ada/code", &paths, "fav")[0];
        assert_eq!((image.name.as_str(), image.folder.as_str()), ("favicon.svg", "apps/web"));
    }

    #[test]
    fn the_folders_above_home_are_typed_from_the_root() {
        assert_eq!(listing(HOME, "", &[], HOME).parent.as_deref(), Some("/home/"));
        assert_eq!(listing("/home", "", &[], HOME).parent.as_deref(), Some("/"));
        assert_eq!(listing("/", "", &["etc".to_string()], HOME).parent, None);
        assert_eq!(listing("/", "", &["etc".to_string()], HOME).folders[0].typed, "/etc/");
        assert_eq!(listing("/home/adam", "", &[], HOME).typed, "/home/adam/");
    }
}
