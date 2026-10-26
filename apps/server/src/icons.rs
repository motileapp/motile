//! The image a project is shown with: a file the user picked, or the first one that looks like
//! an icon in the project's folder.

use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, bail};

use crate::agents::environment::Environment;
use crate::git;

const MAX_ICON: u64 = 1024 * 1024;
const EXTENSIONS: [&str; 7] = ["svg", "png", "ico", "jpg", "jpeg", "webp", "gif"];
/// What an icon is usually called, the most wanted first.
const NAMES: [&str; 9] = [
    "favicon.svg",
    "icon.svg",
    "logo.svg",
    "favicon.png",
    "icon.png",
    "logo.png",
    "apple-touch-icon.png",
    "favicon.ico",
    "icon.ico",
];
/// Where a project usually keeps it, relative to its own folder.
const FOLDERS: [&str; 10] =
    ["", "public", "static", "assets", "app", "src/app", "src/assets", "src", "resources", "docs"];
/// Folders whose children are projects of their own, each with those folders.
const WORKSPACES: [&str; 2] = ["apps", "packages"];
const MAX_WORKSPACE_MEMBERS: usize = 24;
const MAX_LISTED: usize = 5000;
/// How many entries a folder that isn't a repository is walked through at most.
const MAX_WALKED: usize = 50_000;
/// Folders a walk skips, which git would mostly ignore.
const SKIPPED: [&str; 4] = ["node_modules", "target", "build", "dist"];
const LISTING: Duration = Duration::from_secs(10);

fn named_like_image(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    EXTENSIONS.contains(&extension.as_str())
}

pub fn is_icon(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else { return false };
    named_like_image(path) && metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_ICON
}

/// Every image in the project's folder that can be its icon, by its path inside the folder: of
/// what git tracks or doesn't ignore, or in a folder that isn't a repository, what isn't hidden.
pub async fn all(project: &str, environment: &Environment) -> Vec<String> {
    let arguments = ["ls-files", "-z", "--cached", "--others", "--exclude-standard"];
    let listed = git::run(git::command("git", project, environment, &arguments), None, LISTING).await;
    let project = PathBuf::from(project);
    let images = tokio::task::spawn_blocking(move || {
        let paths = match listed {
            Ok(listed) => listed.split('\0').filter(|path| !path.is_empty()).map(str::to_string).collect(),
            Err(_) => walk(&project),
        };
        let mut images: Vec<String> = paths
            .into_iter()
            .filter(|path| named_like_image(Path::new(path)) && is_icon(&project.join(path)))
            .take(MAX_LISTED)
            .collect();
        images.sort();
        images.dedup();
        images
    });
    images.await.unwrap_or_default()
}

/// The files under `root`, by their paths inside it, but for hidden ones and those in `SKIPPED`.
fn walk(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut folders = vec![PathBuf::new()];
    let mut walked = 0;
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&folder)) else { continue };
        for entry in entries.flatten() {
            walked += 1;
            if walked > MAX_WALKED {
                return files;
            }
            let Ok(name) = entry.file_name().into_string() else { continue };
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = folder.join(&name);
            if kind.is_dir() && !SKIPPED.contains(&name.as_str()) {
                folders.push(path);
            } else if kind.is_file() {
                files.push(path.to_string_lossy().into_owned());
            }
        }
    }
    files
}

fn workspace_members(project: &Path) -> Vec<PathBuf> {
    let members = |workspace: &&str| {
        let entries = std::fs::read_dir(project.join(workspace)).ok()?;
        let mut folders: Vec<PathBuf> =
            entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
        folders.sort();
        folders.truncate(MAX_WORKSPACE_MEMBERS);
        Some(folders)
    };
    WORKSPACES.iter().filter_map(members).flatten().collect()
}

/// The first file in the project's folder that is named like an icon.
pub fn find(project: &Path) -> Option<String> {
    let roots: Vec<PathBuf> = std::iter::once(project.to_path_buf()).chain(workspace_members(project)).collect();
    let candidates = NAMES.iter().flat_map(|name| {
        roots.iter().flat_map(move |root| FOLDERS.iter().map(move |folder| root.join(folder).join(name)))
    });
    candidates.into_iter().find(|path| is_icon(path)).map(|path| path.to_string_lossy().into_owned())
}

/// Names the icon's current contents, ending in its extension. `None` when the file can't be
/// used as an icon any more.
pub fn version(path: &str) -> Option<String> {
    let path = Path::new(path);
    if !is_icon(path) {
        return None;
    }
    let metadata = path.metadata().ok()?;
    let changed = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis();
    let extension = path.extension()?.to_str()?.to_lowercase();
    Some(format!("{changed:x}-{:x}.{extension}", metadata.len()))
}

pub fn read(path: &str) -> anyhow::Result<Vec<u8>> {
    if !is_icon(Path::new(path)) {
        bail!("{path} isn't an image of at most 1 MB.");
    }
    std::fs::read(path).with_context(|| format!("{path} can't be read."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(folder: &Path, file: &str) {
        let path = folder.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "<svg/>").unwrap();
    }

    #[test]
    fn the_most_wanted_name_wins_wherever_it_is() {
        let project = tempfile::tempdir().unwrap();
        assert_eq!(find(project.path()), None);

        write(project.path(), "favicon.ico");
        assert_eq!(find(project.path()), Some(project.path().join("favicon.ico").to_string_lossy().into_owned()));

        write(project.path(), "apps/web/public/icon.svg");
        write(project.path(), "public/logo.svg");
        let found = find(project.path()).unwrap();
        assert!(found.ends_with("apps/web/public/icon.svg"), "{found}");

        write(project.path(), "apps/marketing/public/favicon.svg");
        let found = find(project.path()).unwrap();
        assert!(found.ends_with("apps/marketing/public/favicon.svg"), "{found}");
    }

    #[test]
    fn only_a_small_image_file_is_an_icon() {
        let project = tempfile::tempdir().unwrap();
        write(project.path(), "notes.txt");
        std::fs::create_dir(project.path().join("favicon.svg")).unwrap();
        std::fs::write(project.path().join("icon.png"), vec![0u8; MAX_ICON as usize + 1]).unwrap();

        assert_eq!(find(project.path()), None);
        assert_eq!(version(&project.path().join("notes.txt").to_string_lossy()), None);
        assert!(read(&project.path().join("icon.png").to_string_lossy()).is_err());
    }

    #[tokio::test]
    async fn every_image_in_the_folder_is_listed_but_hidden_and_skipped_ones() {
        let project = tempfile::tempdir().unwrap();
        write(project.path(), "logo.svg");
        write(project.path(), "docs/art/banner.png");
        write(project.path(), "notes.txt");
        write(project.path(), ".cache/icon.png");
        write(project.path(), "node_modules/thing/icon.png");

        let variables = std::env::vars().filter(|(name, _)| name == "PATH").collect();
        let environment = Environment::fixed(variables, Default::default());
        let root = project.path().to_string_lossy();
        assert_eq!(all(&root, &environment).await, vec!["docs/art/banner.png", "logo.svg"]);

        std::process::Command::new("git").arg("init").arg("-q").current_dir(project.path()).status().unwrap();
        std::fs::write(project.path().join(".gitignore"), "docs/\n.cache/\nnode_modules/\n").unwrap();
        assert_eq!(all(&root, &environment).await, vec!["logo.svg"]);
    }

    #[test]
    fn the_version_changes_with_the_file() {
        let project = tempfile::tempdir().unwrap();
        write(project.path(), "icon.svg");
        let path = project.path().join("icon.svg").to_string_lossy().into_owned();
        let first = version(&path).unwrap();
        assert!(first.ends_with(".svg"));

        std::fs::write(&path, "<svg><rect/></svg>").unwrap();
        assert_ne!(version(&path).unwrap(), first);

        std::fs::remove_file(&path).unwrap();
        assert_eq!(version(&path), None);
    }
}
