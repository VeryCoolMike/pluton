use std::{os::unix::fs::OpenOptionsExt, str::FromStr, sync::{atomic::{AtomicBool, Ordering}, OnceLock}};
use std::path::PathBuf;
use jiff::{Timestamp, Zoned, tz::TimeZone};
use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use directories::ProjectDirs;

static VERBOSE: AtomicBool = AtomicBool::new(false);
static INFO_LOG: AtomicBool = AtomicBool::new(true);
static PROGRAM_NAME: OnceLock<String> = OnceLock::new();
static LOG_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

static LOG_TX: OnceLock<UnboundedSender<String>> = OnceLock::new();

#[derive(PartialEq, Eq)]
pub enum Importance {
    Info, // Basic telemetry
    Warn, // Something went wrong but it's fine
    Error, // Something serious went wrong
    Fatal // The program has to abort / crash
}

pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::Relaxed);
}

pub fn set_info_log(v: bool) {
    INFO_LOG.store(v, Ordering::Relaxed);
}

pub fn set_log_directory(directory: PathBuf) {
    let _ = LOG_DIRECTORY.set(directory);
}

pub fn set_program_name(name: String) {
    let _ = PROGRAM_NAME.set(name);
}

pub fn default_log_dir() -> Option<PathBuf> {
    let proj = ProjectDirs::from("cc", "Pluton", "pluton")?;

    Some(proj.state_dir().unwrap_or_else(|| proj.data_local_dir()).to_path_buf())
}

pub async fn init_logging() -> std::io::Result<()> {
    let directory = LOG_DIRECTORY.get().unwrap().clone();
    let program_name = PROGRAM_NAME.get().unwrap();

    tokio::fs::create_dir_all(&directory).await?;

    let latest_path = directory.join(format!("latest {}", program_name));

    // If a previous "latest" log already exists, rename it to a timestamped
    // version (based on when it was last written) before starting a new one.
    if let Ok(metadata) = tokio::fs::metadata(&latest_path).await {
        let modified = Timestamp::try_from(metadata.modified()?)
            .map_err(std::io::Error::other)?
            .to_zoned(TimeZone::system());
        let time = modified.strftime("%Y-%m-%d %H-%M-%S").to_string();
        let archived_path = directory.join(format!("{} {}", time, program_name));
        tokio::fs::rename(&latest_path, &archived_path).await?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&latest_path).await?;
    let (tx, mut rx) = unbounded_channel::<String>();
    let _ = LOG_TX.set(tx);
    tokio::spawn(async move {
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            let _ = file.write_all(line.as_bytes()).await;
        }
    });

    Ok(())
}

pub fn pluton_log(msg: &str, level: Importance) {
    let now = Zoned::now();
    let time = now.strftime("%Y-%m-%d %H:%M:%S").to_string();

    let line = match level {
        Importance::Info  => format!("{} [INFO] {msg}", time),
        Importance::Warn  => format!("{} \x1b[0;33m[WARN] {msg}\x1b[0m", time),
        Importance::Error => format!("{} \x1b[0;31m[ERROR] {msg}\x1b[0m", time),
        Importance::Fatal => format!("{} \x1b[1;31m[FATAL] {msg}\x1b[0m", time),
    };

    if VERBOSE.load(Ordering::Relaxed) {
        println!("{line}"); 
    }

    if level == Importance::Info && !INFO_LOG.load(Ordering::Relaxed) {
        return
    }
    
    if let Some(tx) = LOG_TX.get() {
        let line_raw = match level {
            Importance::Info  => format!("{} [INFO] {msg}", time),
            Importance::Warn  => format!("{} [WARN] {msg}", time),
            Importance::Error => format!("{} [ERROR] {msg}", time),
            Importance::Fatal => format!("{} [FATAL] {msg}", time),
        };

        let _ = tx.send(line_raw.to_string());
    }
}
