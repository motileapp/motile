//! Replaces the server's own program with the one in the latest release, and starts it again.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use tokio::io::AsyncWriteExt;

pub const DOWNLOAD_URL: &str = "https://github.com/motileapp/motile/releases/latest/download";
const REPORT_EVERY: Duration = Duration::from_millis(100);

/// The release's file for this kind of machine.
fn archive() -> anyhow::Result<&'static str> {
    archive_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn archive_for(os: &str, arch: &str) -> anyhow::Result<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("motile-x86_64-unknown-linux-musl.tar.gz"),
        ("linux", "aarch64") => Ok("motile-aarch64-unknown-linux-musl.tar.gz"),
        ("macos", "aarch64") => Ok("motile-aarch64-apple-darwin.tar.gz"),
        _ => bail!("There is no release for {arch} machines running {os}."),
    }
}

/// Downloads the latest release from `download_url` and puts its program where `program` is.
/// `progress` hears how many bytes have arrived, and how many there are if the server said.
pub async fn install_latest(
    download_url: &str,
    program: &Path,
    mut progress: impl FnMut(u64, Option<u64>),
) -> anyhow::Result<()> {
    let folder = std::env::temp_dir().join(format!("motile-update-{}", uuid::Uuid::new_v4().simple()));
    tokio::fs::create_dir_all(&folder).await?;
    let installed = async {
        let archive_path = folder.join("motile.tar.gz");
        download(&format!("{download_url}/{}", archive()?), &archive_path, &mut progress).await?;
        let unpacked = tokio::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&folder)
            .status()
            .await
            .context("tar isn't available on this machine.")?;
        if !unpacked.success() {
            bail!("The download couldn't be unpacked.");
        }
        let new_program = folder.join("motile");
        std::fs::set_permissions(&new_program, std::fs::Permissions::from_mode(0o755))
            .context("The release has no motile program in it.")?;
        replace(program, &new_program).await
    };
    let result = installed.await;
    let _ = tokio::fs::remove_dir_all(&folder).await;
    result
}

async fn download(url: &str, to: &Path, progress: &mut impl FnMut(u64, Option<u64>)) -> anyhow::Result<()> {
    motile_protocol::tls::install();
    let mut response = reqwest::get(url)
        .await
        .context("Your server couldn't reach the release. Check its connection and try again.")?;
    if !response.status().is_success() {
        bail!("The release couldn't be downloaded: {}.", response.status());
    }
    let total = response.content_length();
    let mut file = tokio::fs::File::create(to).await?;
    let (mut received, mut reported) = (0u64, Instant::now());
    progress(0, total);
    while let Some(chunk) = response.chunk().await.context("The download was cut off. Try again.")? {
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        if reported.elapsed() >= REPORT_EVERY {
            progress(received, total);
            reported = Instant::now();
        }
    }
    file.flush().await?;
    progress(received, total);
    Ok(())
}

/// A running program can't be written to, but it can be replaced: the new one is put next to
/// it and renamed over it. Where this user may not do that, `sudo` is asked to, without a
/// password.
async fn replace(program: &Path, new_program: &Path) -> anyhow::Result<()> {
    let staged = staging_path(program);
    let direct = std::fs::copy(new_program, &staged).and_then(|_| std::fs::rename(&staged, program));
    let Err(error) = direct else { return Ok(()) };
    let _ = std::fs::remove_file(&staged);
    if error.kind() != std::io::ErrorKind::PermissionDenied {
        return Err(error).with_context(|| format!("{} couldn't be replaced.", program.display()));
    }
    let moved = tokio::process::Command::new("sudo")
        .args(["-n", "mv", "-f"])
        .arg(new_program)
        .arg(program)
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    if !moved.is_ok_and(|status| status.success()) {
        bail!("This server can't replace {}. Run the install command on its machine again.", program.display());
    }
    Ok(())
}

fn staging_path(program: &Path) -> PathBuf {
    program.with_file_name(".motile.new")
}

static RESTART: tokio::sync::Notify = tokio::sync::Notify::const_new();
static PROGRAM: OnceLock<PathBuf> = OnceLock::new();

/// Asks whoever is serving to hang up and start `program` in this process's place.
pub fn request_restart(program: PathBuf) {
    let _ = PROGRAM.set(program);
    RESTART.notify_one();
}

/// Waits until a restart is asked for, and returns the program to start.
pub async fn restart_requested() -> PathBuf {
    RESTART.notified().await;
    PROGRAM.get().cloned().unwrap_or_default()
}

/// Starts `program` in this process's place, with the arguments this process was given.
pub fn restart(program: &Path) -> ! {
    let error = std::process::Command::new(program).args(std::env::args_os().skip(1)).exec();
    tracing::error!("couldn't start the updated server: {error}");
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncReadExt;

    use super::*;

    /// Serves `body` to every request, as a release download would.
    async fn serve(body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let body = body.clone();
                tokio::spawn(async move {
                    let mut request = [0u8; 2048];
                    let _ = stream.read(&mut request).await;
                    let header =
                        format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
                    let _ = stream.write_all(header.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                });
            }
        });
        format!("http://{address}")
    }

    fn release_with(program: &str) -> Vec<u8> {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("motile"), program).unwrap();
        let packed =
            std::process::Command::new("tar").args(["-czf", "release.tar.gz", "motile"]).current_dir(&folder).status();
        assert!(packed.unwrap().success());
        std::fs::read(folder.path().join("release.tar.gz")).unwrap()
    }

    #[tokio::test]
    async fn the_latest_release_takes_the_programs_place_and_says_how_far_it_is() {
        let folder = tempfile::tempdir().unwrap();
        let program = folder.path().join("motile");
        std::fs::write(&program, "old").unwrap();
        let release = release_with("#!/bin/sh\necho new\n");
        let url = serve(release.clone()).await;
        let mut reports = Vec::new();

        install_latest(&url, &program, |received, total| reports.push((received, total))).await.unwrap();

        assert_eq!(std::fs::read_to_string(&program).unwrap(), "#!/bin/sh\necho new\n");
        assert_eq!(std::fs::metadata(&program).unwrap().permissions().mode() & 0o777, 0o755);
        let size = release.len() as u64;
        assert_eq!(reports.first(), Some(&(0, Some(size))));
        assert_eq!(reports.last(), Some(&(size, Some(size))));
        assert!(!staging_path(&program).exists());
    }

    #[test]
    fn each_kind_of_machine_gets_its_own_release_file() {
        assert_eq!(archive_for("linux", "x86_64").unwrap(), "motile-x86_64-unknown-linux-musl.tar.gz");
        assert_eq!(archive_for("linux", "aarch64").unwrap(), "motile-aarch64-unknown-linux-musl.tar.gz");
        assert_eq!(archive_for("macos", "aarch64").unwrap(), "motile-aarch64-apple-darwin.tar.gz");
        assert!(archive_for("macos", "x86_64").is_err());
    }

    #[tokio::test]
    async fn a_download_that_is_not_a_release_leaves_the_program_alone() {
        let folder = tempfile::tempdir().unwrap();
        let program = folder.path().join("motile");
        std::fs::write(&program, "old").unwrap();
        let url = serve(b"not an archive".to_vec()).await;

        let failed = install_latest(&url, &program, |_, _| {}).await;

        assert!(failed.is_err());
        assert_eq!(std::fs::read_to_string(&program).unwrap(), "old");
    }
}
