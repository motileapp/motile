//! Off the Mac, where no system hands an address to the app that already runs: the system is told
//! to open `motile://` addresses with this program, and one copy of the client runs at a time. A
//! second one, as the browser starts at the end of a sign-in, hands what it was opened with to the
//! first, which comes to the front, and quits.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use futures::channel::mpsc::UnboundedSender;
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

const SCHEME: &str = "motile";

/// The `motile://` addresses this launch was opened with.
pub fn opened_urls() -> Vec<String> {
    let prefix = format!("{SCHEME}://");
    std::env::args().skip(1).filter(|arg| arg.starts_with(&prefix)).collect()
}

/// Hands the addresses to the copy that runs, if one does. Whether it took them.
pub fn hand_over(urls: &[String]) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket()) else { return false };
    let lines: String = urls.iter().map(|url| format!("{url}\n")).collect();
    stream.write_all(lines.as_bytes()).is_ok()
}

/// Passes on what every later launch hands over: its addresses, none when it was only started.
pub fn listen(opened: UnboundedSender<Vec<String>>) {
    let socket = socket();
    if let Some(folder) = socket.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    // What is left of a copy that didn't quit cleanly: no copy answered on it.
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::warn!("later launches can't reach this one: {error}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let urls = BufReader::new(stream).lines().map_while(Result::ok).filter(|line| !line.is_empty()).collect();
            if opened.unbounded_send(urls).is_err() {
                break;
            }
        }
    });
}

fn socket() -> PathBuf {
    crate::store::data_dir().join("instance.sock")
}

/// Registers the program and quits, for the installer (`apps/marketing/public/linux`).
#[cfg(target_os = "linux")]
pub const REGISTER: &str = "--register";

/// Has the desktop open `motile://` addresses with this program, and list it with its icon.
#[cfg(target_os = "linux")]
pub fn register_scheme() {
    const ICON: &[u8] = include_bytes!("../assets/icon.png");
    const NAME: &str = "app.motile.motile";
    const FORMER: &str = "app.motile.gpui";

    let Ok(program) = std::env::current_exe() else { return };
    let Some(share) = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
    else {
        return;
    };
    let icon = share.join(format!("icons/hicolor/512x512/apps/{NAME}.png"));
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Motile\nComment=The command center for coding agents\n\
         Exec={} %u\nIcon={NAME}\nTerminal=false\nCategories=Development;\nMimeType=x-scheme-handler/{SCHEME};\n\
         StartupWMClass={NAME}\n",
        desktop_quoted(&program.to_string_lossy())
    );
    let applications = share.join("applications");
    let _ = std::fs::remove_file(applications.join(format!("{FORMER}.desktop")));
    let _ = std::fs::remove_file(share.join(format!("icons/hicolor/512x512/apps/{FORMER}.png")));
    let file = applications.join(format!("{NAME}.desktop"));
    let written = |path: &std::path::Path, bytes: &[u8]| -> bool {
        if std::fs::read(path).is_ok_and(|kept| kept == bytes) {
            return false;
        }
        path.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
            && std::fs::write(path, bytes).is_ok()
    };
    written(&icon, ICON);
    if !written(&file, entry.as_bytes()) {
        return;
    }
    let _ = std::process::Command::new("update-desktop-database").arg(&applications).status();
    let _ = std::process::Command::new("xdg-mime")
        .args(["default", &format!("{NAME}.desktop"), &format!("x-scheme-handler/{SCHEME}")])
        .status();
}

/// A path as a desktop entry's `Exec` takes it: quoted, with what is special in quotes escaped.
#[cfg(target_os = "linux")]
fn desktop_quoted(path: &str) -> String {
    let mut quoted = String::from("\"");
    for character in path.chars() {
        match character {
            '"' | '`' | '$' | '\\' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

/// Has Windows open `motile://` addresses with this program, for this user.
#[cfg(windows)]
pub fn register_scheme() {
    use windows_registry::CURRENT_USER;

    let Ok(program) = std::env::current_exe() else { return };
    let program = program.to_string_lossy();
    let key = format!(r"Software\Classes\{SCHEME}");
    let registered = (|| -> windows_registry::Result<()> {
        let scheme = CURRENT_USER.create(&key)?;
        scheme.set_string("", "URL:Motile")?;
        scheme.set_string("URL Protocol", "")?;
        CURRENT_USER.create(format!(r"{key}\DefaultIcon"))?.set_string("", format!("\"{program}\",0"))?;
        CURRENT_USER.create(format!(r"{key}\shell\open\command"))?.set_string("", format!("\"{program}\" \"%1\""))?;
        Ok(())
    })();
    if let Err(error) = registered {
        tracing::warn!("motile:// addresses won't open the client: {error}");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::desktop_quoted;

    #[test]
    fn a_path_is_quoted_for_a_desktop_entry() {
        assert_eq!(desktop_quoted("/opt/Motile GPUI/motile-gpui"), "\"/opt/Motile GPUI/motile-gpui\"");
        assert_eq!(desktop_quoted("/home/a$b/100%"), "\"/home/a\\\\$b/100%%\"");
    }
}
