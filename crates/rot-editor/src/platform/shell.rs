//! Launch the host desktop's file manager on a validated folder path.
use std::io;
use std::path::Path;

/// Open `path` in the OS file manager without waiting on the external viewer.
pub fn open_in_file_manager(path: &Path) -> io::Result<()> {
    let program = {
        #[cfg(target_os = "linux")]
        {
            "xdg-open"
        }
        #[cfg(target_os = "macos")]
        {
            "open"
        }
        #[cfg(windows)]
        {
            "explorer"
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            let _ = path;
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Opening the file manager is not yet supported on this platform.",
            ));
        }
    };
    let mut child = std::process::Command::new(program).arg(path).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
