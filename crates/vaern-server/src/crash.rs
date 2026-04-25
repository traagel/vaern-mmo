//! Server-side panic hook + crash log writer.
//!
//! Installed in `main` before any Bevy plugin spins up. On panic the hook:
//!   1. delegates to the default panic hook (so stderr / journalctl still
//!      see the message)
//!   2. writes a crash report to
//!      `~/.local/share/vaern/server/crash_<unix_ts>.log` with the panic
//!      message, source location, thread name, captured backtrace,
//!      timestamp, and git SHA if compiled with one.
//!
//! Failure to write the crash file is itself swallowed and reported via
//! `eprintln!` — we never want a panic in the panic hook.

use std::backtrace::Backtrace;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const CRASH_DIR_REL: &str = ".local/share/vaern/server";

/// Install the global panic hook. Idempotent in practice — calling twice
/// just chains hooks; only intended to be called once from `main`.
pub fn install() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Capture before we hand off — the default hook may print the
        // shorter form. We always write a forced backtrace to file.
        let backtrace = Backtrace::force_capture();
        let report = build_report(info, &backtrace);
        if let Err(e) = write_report(&report) {
            eprintln!("[crash hook] failed to write crash log: {e}");
        }
        default_hook(info);
    }));
}

fn build_report(info: &std::panic::PanicHookInfo<'_>, backtrace: &Backtrace) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let payload = panic_payload(info);
    let thread = std::thread::current()
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{:?}", std::thread::current().id()));
    let git_sha = option_env!("VAERN_GIT_SHA").unwrap_or("unknown");
    let pkg_version = env!("CARGO_PKG_VERSION");

    format!(
        "vaern-server crash report\n\
         ─────────────────────────\n\
         timestamp (unix): {ts}\n\
         pkg_version: {pkg_version}\n\
         git_sha: {git_sha}\n\
         thread: {thread}\n\
         location: {location}\n\
         message: {payload}\n\
         \n\
         backtrace:\n{backtrace}\n"
    )
}

fn panic_payload(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

fn write_report(report: &str) -> std::io::Result<PathBuf> {
    let dir = crash_dir()?;
    fs::create_dir_all(&dir)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("crash_{ts}.log"));
    let mut f = fs::File::create(&path)?;
    f.write_all(report.as_bytes())?;
    f.flush()?;
    eprintln!("[crash hook] wrote {}", path.display());
    Ok(path)
}

fn crash_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "$HOME not set; cannot pick crash log directory",
        )
    })?;
    Ok(PathBuf::from(home).join(CRASH_DIR_REL))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_dir_uses_home_env() {
        let prev = std::env::var_os("HOME");
        // SAFETY: tests are single-threaded enough; restore at end.
        unsafe {
            std::env::set_var("HOME", "/tmp/vaern-crash-test");
        }
        let dir = crash_dir().expect("HOME set");
        assert!(dir.ends_with(".local/share/vaern/server"));
        assert!(dir.starts_with("/tmp/vaern-crash-test"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}
