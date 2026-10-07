//! Browsing a server's folders by typing a path, the way a shell completes one: the folders of
//! the directory typed so far, narrowed by what follows its last slash, and the images that can
//! be a project's icon when one is being chosen. Every client shows the same.

use serde::Serialize;

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
    pub images: Vec<Image>,
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

/// The folders and images of `directory` whose names start with `leaf`, whatever the case.
pub fn listing(directory: &str, leaf: &str, folders: &[String], images: &[String], home: &str) -> Listing {
    let leaf = leaf.to_lowercase();
    let base = directory.trim_end_matches('/');
    let named = |name: &&String| name.to_lowercase().starts_with(&leaf);
    let folder = |name: &String| {
        let path = format!("{base}/{name}");
        Folder { name: name.clone(), typed: as_typed(&path, home), path }
    };
    let image = |name: &String| Image { name: name.clone(), path: format!("{base}/{name}") };
    let parent = (directory != "/").then(|| as_typed(base.rsplit_once('/').map_or("", |(parent, _)| parent), home));
    Listing {
        path: directory.to_string(),
        typed: as_typed(directory, home),
        parent,
        folders: folders.iter().filter(named).map(folder).collect(),
        images: images.iter().filter(named).map(image).collect(),
    }
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
        let listed = listing("/home/ada/code", "mo", &folders, &[], HOME);

        assert_eq!(listed.typed, "~/code/");
        assert_eq!(listed.parent.as_deref(), Some("~/"));
        let found: Vec<_> = listed.folders.iter().map(|folder| (folder.path.as_str(), folder.typed.as_str())).collect();
        assert_eq!(found, vec![("/home/ada/code/Motile", "~/code/Motile/"), ("/home/ada/code/more", "~/code/more/")]);
    }

    #[test]
    fn images_are_narrowed_by_the_name_too() {
        let images = ["Logo.png".to_string(), "icon.svg".to_string()];
        let listed = listing("/home/ada/code/", "lo", &[], &images, HOME);

        assert_eq!(listed.images, vec![Image { name: "Logo.png".into(), path: "/home/ada/code/Logo.png".into() }]);
    }

    #[test]
    fn the_folders_above_home_are_typed_from_the_root() {
        assert_eq!(listing(HOME, "", &[], &[], HOME).parent.as_deref(), Some("/home/"));
        assert_eq!(listing("/home", "", &[], &[], HOME).parent.as_deref(), Some("/"));
        assert_eq!(listing("/", "", &["etc".to_string()], &[], HOME).parent, None);
        assert_eq!(listing("/", "", &["etc".to_string()], &[], HOME).folders[0].typed, "/etc/");
        assert_eq!(listing("/home/adam", "", &[], &[], HOME).typed, "/home/adam/");
    }
}
