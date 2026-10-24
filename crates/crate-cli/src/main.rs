//! `crate`: the command-line companion. Everything archive-related happens in
//! `crate-core`; this file only parses arguments and prints.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use crate_core::{
    Archive, CancelToken, EntryKind, Error, ExtractOptions, IfExists, NameCodec, OpenOptions,
    Progress, Selection,
};

const USAGE: &str = "\
Usage: crate <command> <archive> [options]

Commands:
  l <archive>    list the contents
  x <archive>    extract (into a folder named after the archive by default)
  t <archive>    test integrity

Options:
  -o, --output <dir>      extract into <dir>
  --encoding <name>       decode old file names with this encoding
                          (cp1256, cp437, cp866, shift-jis, gbk, cp1251…)
  -p, --password <pw>     password for encrypted entries (other users on
                          this machine can see command lines)
  --overwrite             replace existing files (default: keep them)
  --allow-high-ratio      extract even if the archive expands suspiciously much
  -h, --help              show this help
  -V, --version           show the version";

/// Exit code for "stopped by Ctrl+C", as shells use for SIGINT.
const CANCELLED: u8 = 130;

struct Args {
    command: String,
    archive: PathBuf,
    output: Option<PathBuf>,
    encoding: Option<NameCodec>,
    password: Option<String>,
    overwrite: bool,
    allow_high_ratio: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter();
    let command = it.next().ok_or("missing command")?.clone();
    if !matches!(command.as_str(), "l" | "x" | "t") {
        return Err(format!("unknown command `{command}`"));
    }
    let mut archive = None;
    let mut parsed = Args {
        command,
        archive: PathBuf::new(),
        output: None,
        encoding: None,
        password: None,
        overwrite: false,
        allow_high_ratio: false,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                parsed.output = Some(it.next().ok_or("missing folder after -o")?.into())
            }
            "--encoding" => {
                let label = it.next().ok_or("missing name after --encoding")?;
                parsed.encoding = Some(
                    NameCodec::from_label(label)
                        .ok_or_else(|| Error::UnknownEncoding(label.clone()).to_string())?,
                );
            }
            "-p" | "--password" => {
                parsed.password = Some(it.next().ok_or("missing password after -p")?.clone())
            }
            "--overwrite" => parsed.overwrite = true,
            "--allow-high-ratio" => parsed.allow_high_ratio = true,
            a if a.starts_with('-') => return Err(format!("unknown option `{a}`")),
            a if archive.is_none() => archive = Some(PathBuf::from(a)),
            a => return Err(format!("unexpected argument `{a}`")),
        }
    }
    parsed.archive = archive.ok_or("missing archive")?;
    Ok(parsed)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version") => {
            println!("crate {}", crate_core::version());
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let args = match parse(&args) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("crate: {msg}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let cancel = CancelToken::new();
    let c = cancel.clone();
    let _ = ctrlc::set_handler(move || c.cancel());

    let result = Archive::open_with(
        &args.archive,
        &OpenOptions {
            encoding: args.encoding,
            password: args.password.clone(),
        },
    )
    .and_then(|mut archive| match args.command.as_str() {
        "l" => list(&archive),
        "x" => extract(&mut archive, &args, &cancel),
        _ => test(&mut archive, &cancel),
    });
    match result {
        Ok(code) => code,
        Err(Error::Cancelled) => {
            clear_progress();
            eprintln!("crate: cancelled; nothing was written");
            ExitCode::from(CANCELLED)
        }
        Err(e) => {
            clear_progress();
            eprintln!("crate: {}: {e}", args.archive.display());
            ExitCode::FAILURE
        }
    }
}

fn note_encoding(archive: &Archive) {
    if let Some(enc) = archive.name_encoding()
        && enc.guessed
    {
        eprintln!(
            "crate: file names are not UTF-8; decoded as {} (use --encoding to change)",
            enc.codec.name()
        );
    }
}

fn list(archive: &Archive) -> crate_core::Result<ExitCode> {
    note_encoding(archive);
    let mut out = std::io::stdout().lock();
    let _ = writeln!(
        out,
        "{:>12}  {:>12}  {:16}  Name",
        "Size", "Packed", "Modified"
    );
    let (mut files, mut total) = (0u64, 0u64);
    for e in archive.entries() {
        let packed = e.packed_size.map(|p| p.to_string()).unwrap_or_default();
        let modified = e.modified.map(format_time).unwrap_or_default();
        let name = match &e.kind {
            EntryKind::Symlink { target } | EntryKind::Hardlink { target } => {
                format!("{} -> {target}", e.path)
            }
            _ => e.path.clone(),
        };
        let size = if e.is_dir() {
            String::new()
        } else {
            e.size.to_string()
        };
        let lock = if e.encrypted { " (encrypted)" } else { "" };
        let _ = writeln!(out, "{size:>12}  {packed:>12}  {modified:16}  {name}{lock}");
        if e.kind == EntryKind::File {
            files += 1;
            total += e.size;
        }
    }
    let _ = writeln!(
        out,
        "{files} files, {total} bytes ({})",
        archive.format().name()
    );
    Ok(ExitCode::SUCCESS)
}

fn extract(
    archive: &mut Archive,
    args: &Args,
    cancel: &CancelToken,
) -> crate_core::Result<ExitCode> {
    note_encoding(archive);
    let dest = args
        .output
        .clone()
        .unwrap_or_else(|| default_folder(&args.archive));
    let mut options = ExtractOptions::default();
    if args.overwrite {
        options.if_exists = IfExists::Overwrite;
    }
    if args.allow_high_ratio {
        options.limits.max_ratio = None;
    }
    let report = archive.extract(&Selection::All, &dest, &options, &mut show_progress, cancel)?;
    clear_progress();
    let files = if report.files == 1 { "file" } else { "files" };
    println!(
        "Extracted {} {files} ({} bytes) to {}",
        report.files,
        report.bytes,
        dest.display()
    );
    if !report.skipped.is_empty() {
        let n = report.skipped.len();
        eprintln!(
            "{n} item{} skipped:",
            if n == 1 { " was" } else { "s were" }
        );
        for s in &report.skipped {
            eprintln!("  {}: {}", s.path, s.reason);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn test(archive: &mut Archive, cancel: &CancelToken) -> crate_core::Result<ExitCode> {
    note_encoding(archive);
    let report = archive.test(&mut show_progress, cancel)?;
    clear_progress();
    for s in &report.skipped {
        eprintln!("  {}: not tested ({})", s.path, s.reason);
    }
    for (path, error) in &report.failed {
        eprintln!("  {path}: {error}");
    }
    if report.is_ok() {
        println!("OK: {} files, {} bytes", report.files, report.bytes);
        Ok(ExitCode::SUCCESS)
    } else {
        println!(
            "FAILED: {} of {} files damaged",
            report.failed.len(),
            report.files + report.failed.len() as u64
        );
        Ok(ExitCode::FAILURE)
    }
}

/// `photos.tar.gz` → `photos/` next to the current folder.
fn default_folder(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let suffixes = [
        ".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tgz", ".txz", ".tzst", ".tbz2", ".tar",
        ".zip", ".7z", ".rar",
    ];
    let stem = suffixes
        .iter()
        .find(|s| lower.ends_with(*s) && lower.len() > s.len())
        .map_or(name.as_str(), |s| &name[..name.len() - s.len()]);
    PathBuf::from(if stem.is_empty() { "extracted" } else { stem })
}

fn show_progress(p: &Progress) {
    let mut err = std::io::stderr();
    if !err.is_terminal() || p.bytes_total == 0 {
        return;
    }
    let pct = p.bytes_done * 100 / p.bytes_total;
    let _ = write!(
        err,
        "\r\x1b[K{pct:3}%  {}/{}  {}",
        p.files_done, p.files_total, p.current
    );
    let _ = err.flush();
}

fn clear_progress() {
    let mut err = std::io::stderr();
    if err.is_terminal() {
        let _ = write!(err, "\r\x1b[K");
    }
}

/// `YYYY-MM-DD HH:MM` in UTC (enough for a listing; no time-zone database).
fn format_time(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_folder_names() {
        assert_eq!(
            default_folder(Path::new("dl/photos.tar.gz")),
            Path::new("photos")
        );
        assert_eq!(default_folder(Path::new("Report.ZIP")), Path::new("Report"));
        assert_eq!(default_folder(Path::new("a.b.7z")), Path::new("a.b"));
        assert_eq!(default_folder(Path::new("noext")), Path::new("noext"));
        assert_eq!(default_folder(Path::new(".zip")), Path::new(".zip"));
    }

    #[test]
    fn time_format() {
        assert_eq!(
            format_time(UNIX_EPOCH + std::time::Duration::from_secs(1_792_845_015)),
            "2026-10-24 12:30"
        );
    }
}
