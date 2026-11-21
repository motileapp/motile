//! The pinned release of Cua Driver: downloaded once into the server's folder, checked against its
//! size and SHA-256 as it arrives, and unpacked beside the one before it, which goes once it's in
//! place.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub const VERSION: &str = "0.55.0";
const DOWNLOAD_URL: &str = "https://github.com/trycua/cua/releases/download";
/// Longer than the slowest connection takes for its 45 MB, which never counts against starting.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// What is taken from the archive: the driver, and the helper that draws the agent's cursor.
const PROGRAMS: [&str; 2] = ["cua-driver", "cua-cursor-theme"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Archive {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// The archive for this kind of machine.
pub fn archive() -> anyhow::Result<Archive> {
    archive_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn archive_for(os: &str, arch: &str) -> anyhow::Result<Archive> {
    let (name, size, sha256) = match (os, arch) {
        ("macos", _) => (
            "cua-driver-rs-0.55.0-darwin-universal-binary.tar.gz",
            43_825_788,
            "e6246d3da20abe019e2660e8b5f77e0c12cf9b386597a76393116994a36db6ce",
        ),
        ("linux", "x86_64") => (
            "cua-driver-rs-0.55.0-linux-x86_64-binary.tar.gz",
            30_235_418,
            "1cb8aa6d30e5cdf5bf4d222cb3693a50af2c178073a49dc59e88737c5f7807e6",
        ),
        ("linux", "aarch64") => (
            "cua-driver-rs-0.55.0-linux-arm64-binary.tar.gz",
            28_132_426,
            "401f7f4a9ad276dbcdb324f1b00e152987a46985b68c9e625ac01101f5e6af77",
        ),
        _ => bail!("Computer use isn't available on {arch} machines running {os}."),
    };
    Ok(Archive { name, size, sha256 })
}

/// The driver's program, downloaded into `folder` the first time.
pub async fn install(folder: &Path) -> anyhow::Result<PathBuf> {
    let url = format!("{DOWNLOAD_URL}/cua-driver-rs-v{VERSION}/{}", archive()?.name);
    install_from(&url, archive()?, folder).await
}

async fn install_from(url: &str, archive: Archive, folder: &Path) -> anyhow::Result<PathBuf> {
    let installed = folder.join(format!("cua-driver-{VERSION}"));
    let program = installed.join("cua-driver");
    if program.is_file() {
        return Ok(program);
    }
    tokio::fs::create_dir_all(folder).await?;
    let unfinished = folder.join(format!(".unfinished-{}", uuid::Uuid::new_v4().simple()));
    tokio::fs::create_dir_all(&unfinished).await?;
    let unpacked = async {
        let downloaded = unfinished.join("driver.tar.gz");
        tokio::time::timeout(DOWNLOAD_TIMEOUT, download(url, archive, &downloaded))
            .await
            .context("Downloading the computer use driver took too long. Try again.")??;
        let unpacked = tokio::process::Command::new("tar")
            .arg("-xzf")
            .arg(&downloaded)
            .arg("-C")
            .arg(&unfinished)
            .args(PROGRAMS)
            .status()
            .await
            .context("tar isn't available on this machine.")?;
        if !unpacked.success() {
            bail!("The computer use driver couldn't be unpacked.");
        }
        tokio::fs::remove_file(&downloaded).await?;
        for name in PROGRAMS {
            std::fs::set_permissions(unfinished.join(name), std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("The computer use driver's release has no {name} in it."))?;
        }
        Ok(())
    };
    if let Err(error) = unpacked.await {
        let _ = tokio::fs::remove_dir_all(&unfinished).await;
        return Err(error);
    }
    tokio::fs::rename(&unfinished, &installed).await?;
    remove_others(folder, &installed);
    Ok(program)
}

/// Downloads `archive` from `url` to `to`, refusing it as soon as it differs from what was pinned.
async fn download(url: &str, archive: Archive, to: &Path) -> anyhow::Result<()> {
    motile_protocol::tls::install();
    let mut response = reqwest::get(url)
        .await
        .context("Your server couldn't reach the computer use driver's release. Check its connection and try again.")?;
    if !response.status().is_success() {
        bail!("The computer use driver couldn't be downloaded: {}.", response.status());
    }
    let mut file = tokio::fs::File::create(to).await?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    while let Some(chunk) = response.chunk().await.context("The download was cut off. Try again.")? {
        received += chunk.len() as u64;
        if received > archive.size {
            bail!("The computer use driver's download is larger than its release.");
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    if received != archive.size || hex::encode(hasher.finalize()) != archive.sha256 {
        bail!("The computer use driver's download isn't the release it should be.");
    }
    Ok(())
}

/// Removes the versions before `installed`, and what an interrupted install left.
fn remove_others(folder: &Path, installed: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let left = name.starts_with("cua-driver-") || name.starts_with(".unfinished-");
        if left && path != installed && path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_with(programs: &[(&str, &str)]) -> Vec<u8> {
        let folder = tempfile::tempdir().unwrap();
        for (name, text) in programs {
            std::fs::write(folder.path().join(name), text).unwrap();
        }
        let names: Vec<&str> = programs.iter().map(|(name, _)| *name).collect();
        let packed = std::process::Command::new("tar")
            .args(["-czf", "release.tar.gz"])
            .args(&names)
            .current_dir(folder.path())
            .status()
            .unwrap();
        assert!(packed.success());
        std::fs::read(folder.path().join("release.tar.gz")).unwrap()
    }

    async fn serve(body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/driver.tar.gz", axum::routing::get(move || async move { body.clone() }));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}/driver.tar.gz")
    }

    fn pinned(bytes: &[u8]) -> Archive {
        let sha256 = hex::encode(Sha256::digest(bytes)).leak();
        Archive { name: "driver.tar.gz", size: bytes.len() as u64, sha256 }
    }

    #[tokio::test]
    async fn the_pinned_release_is_unpacked_once_and_replaces_the_one_before() {
        let bytes = archive_with(&[("cua-driver", "driver"), ("cua-cursor-theme", "theme"), ("README", "not taken")]);
        let url = serve(bytes.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let old = folder.path().join("cua-driver-0.1.0");
        std::fs::create_dir_all(&old).unwrap();

        let program = install_from(&url, pinned(&bytes), folder.path()).await.unwrap();

        assert_eq!(program, folder.path().join(format!("cua-driver-{VERSION}/cua-driver")));
        assert_eq!(std::fs::read_to_string(&program).unwrap(), "driver");
        assert_eq!(std::fs::metadata(&program).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(!program.with_file_name("README").exists());
        assert!(!old.exists());
        let again = install_from("http://127.0.0.1:9/gone", pinned(&bytes), folder.path()).await.unwrap();
        assert_eq!(again, program);
    }

    #[tokio::test]
    async fn a_download_that_is_not_the_pinned_release_is_refused_and_leaves_nothing() {
        let bytes = archive_with(&[("cua-driver", "driver"), ("cua-cursor-theme", "theme")]);
        let url = serve(bytes.clone()).await;
        let folder = tempfile::tempdir().unwrap();
        let mut other = pinned(&bytes);
        other.sha256 = "0000000000000000000000000000000000000000000000000000000000000000";

        let error = install_from(&url, other, folder.path()).await.unwrap_err();
        assert!(error.to_string().contains("isn't the release"), "{error:#}");
        let shorter = Archive { size: bytes.len() as u64 - 1, ..pinned(&bytes) };
        let error = install_from(&url, shorter, folder.path()).await.unwrap_err();
        assert!(error.to_string().contains("larger than its release"), "{error:#}");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[test]
    fn each_kind_of_server_gets_its_own_archive() {
        assert!(archive_for("macos", "aarch64").unwrap().name.ends_with("darwin-universal-binary.tar.gz"));
        assert!(archive_for("linux", "x86_64").unwrap().name.ends_with("linux-x86_64-binary.tar.gz"));
        assert!(archive_for("linux", "aarch64").unwrap().name.ends_with("linux-arm64-binary.tar.gz"));
        assert!(archive_for("windows", "x86_64").is_err());
        for archive in [archive_for("macos", "aarch64"), archive_for("linux", "x86_64")] {
            assert!(archive.unwrap().name.contains(VERSION));
        }
    }
}
