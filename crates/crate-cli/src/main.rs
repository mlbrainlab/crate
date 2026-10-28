//! `crate`: the command-line companion. Everything archive-related happens in
//! `crate-core`; this file only parses arguments and prints.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use crate_core::create::{self, CreateOptions, OutFormat};
use crate_core::edit::{self, Change, EditOptions};
use crate_core::{
    Archive, CancelToken, EntryKind, Error, ExtractOptions, IfExists, NameCodec, OpenOptions,
    Progress, Selection, checksum, convert,
};

const USAGE: &str = "\
Usage: crate <command> <archive> [arguments] [options]

Commands:
  l <archive>                    list the contents
  x <archive>                    extract (into a folder named after the archive)
  t <archive>                    test integrity
  c <new archive> <files...>     create; the format comes from the name:
                                 .zip .7z .tar .tar.gz .tar.xz .tar.zst .tar.bz2
                                 (add .age to a tar name to encrypt it)
  a <archive> <files...>         add files and folders (see --into)
  d <archive> <paths...>         delete files or folders inside the archive
  rn <archive> <from> <to>       rename a file or folder inside the archive
  sum <archive>                  SHA-256, BLAKE3 and MD5 of the archive file
  convert <archive> <new archive>
                                 copy everything into another format

Options:
  -o, --output <dir>      extract into <dir>
  --encoding <name>       decode old file names with this encoding
                          (cp1256, cp437, cp866, shift-jis, gbk, cp1251…)
  -p, --password <pw>     password (other users on this machine can see
                          command lines); for c and convert: encrypt with it
  --source-password <pw>  convert: the password of the archive being read
  --overwrite             replace existing files (default: keep them)
  --allow-high-ratio      extract even if the archive expands suspiciously much
  --level <n>             compression level (zstd 1-19, others 0-9)
  --exclude <pattern>     leave out matching files (*.tmp, .git/); repeatable
  --volume <size>         split into volumes of <size> (700M, 4G)
  --no-permissions        don't keep Unix permissions and symlinks
  --into <folder>         a: add into this folder of the archive
  --backup                a, d, rn: keep the previous version as <name>.bak
  -h, --help              show this help
  -V, --version           show the version";

/// Exit code for "stopped by Ctrl+C", as shells use for SIGINT.
const CANCELLED: u8 = 130;

struct Args {
    command: String,
    archive: PathBuf,
    /// Everything after the archive.
    rest: Vec<String>,
    output: Option<PathBuf>,
    encoding: Option<NameCodec>,
    password: Option<String>,
    source_password: Option<String>,
    overwrite: bool,
    allow_high_ratio: bool,
    level: Option<u32>,
    exclude: Vec<String>,
    volume: Option<u64>,
    keep_permissions: bool,
    into: String,
    backup: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter();
    let command = it.next().ok_or("missing command")?.clone();
    let (min, max) = match command.as_str() {
        "l" | "x" | "t" | "sum" => (0, 0),
        "c" | "a" | "d" => (1, usize::MAX),
        "rn" => (2, 2),
        "convert" => (1, 1),
        _ => return Err(format!("unknown command `{command}`")),
    };
    let mut positional = Vec::new();
    let mut parsed = Args {
        command,
        archive: PathBuf::new(),
        rest: Vec::new(),
        output: None,
        encoding: None,
        password: None,
        source_password: None,
        overwrite: false,
        allow_high_ratio: false,
        level: None,
        exclude: Vec::new(),
        volume: None,
        keep_permissions: true,
        into: String::new(),
        backup: false,
    };
    let value = |it: &mut std::slice::Iter<String>, what: &str| -> Result<String, String> {
        it.next().cloned().ok_or(format!("missing {what}"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" | "--output" => parsed.output = Some(value(&mut it, "folder after -o")?.into()),
            "--encoding" => {
                let label = value(&mut it, "name after --encoding")?;
                parsed.encoding = Some(
                    NameCodec::from_label(&label)
                        .ok_or_else(|| Error::UnknownEncoding(label.clone()).to_string())?,
                );
            }
            "-p" | "--password" => parsed.password = Some(value(&mut it, "password after -p")?),
            "--source-password" => {
                parsed.source_password = Some(value(&mut it, "password after --source-password")?)
            }
            "--overwrite" => parsed.overwrite = true,
            "--allow-high-ratio" => parsed.allow_high_ratio = true,
            "--level" => {
                let v = value(&mut it, "number after --level")?;
                parsed.level = Some(v.parse().map_err(|_| format!("bad level `{v}`"))?);
            }
            "--exclude" => parsed
                .exclude
                .push(value(&mut it, "pattern after --exclude")?),
            "--volume" => {
                let v = value(&mut it, "size after --volume")?;
                parsed.volume = Some(parse_size(&v).ok_or(format!("bad size `{v}`"))?);
            }
            "--no-permissions" => parsed.keep_permissions = false,
            "--into" => parsed.into = value(&mut it, "folder after --into")?,
            "--backup" => parsed.backup = true,
            a if a.starts_with('-') && a.len() > 1 => return Err(format!("unknown option `{a}`")),
            a => positional.push(a.to_owned()),
        }
    }
    let mut positional = positional.into_iter();
    parsed.archive = positional.next().ok_or("missing archive")?.into();
    parsed.rest = positional.collect();
    if parsed.rest.len() < min {
        return Err(format!("`{}` needs more arguments", parsed.command));
    }
    if parsed.rest.len() > max {
        return Err(format!("unexpected argument `{}`", parsed.rest[max]));
    }
    Ok(parsed)
}

/// `700M`, `4G`, `1.5G`, `4096` (bytes).
fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let (number, unit) = match text.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        Some((i, _)) => text.split_at(i),
        None => (text, ""),
    };
    let n: f64 = number.trim().parse().ok()?;
    let mult = match unit
        .to_ascii_uppercase()
        .trim_end_matches("IB")
        .trim_end_matches('B')
    {
        "" => 1.0,
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    (n > 0.0).then_some((n * mult) as u64)
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

    let result = match args.command.as_str() {
        "c" => create_archive(&args, &cancel),
        "a" | "d" | "rn" => edit_archive(&args, &cancel),
        "sum" => checksums(&args, &cancel),
        "convert" => convert_archive(&args, &cancel),
        _ => Archive::open_with(
            &args.archive,
            &OpenOptions {
                encoding: args.encoding,
                password: args.password.clone(),
                ..OpenOptions::default()
            },
        )
        .and_then(|mut archive| match args.command.as_str() {
            "l" => list(&archive),
            "x" => extract(&mut archive, &args, &cancel),
            _ => test(&mut archive, &cancel),
        }),
    };
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

fn create_archive(args: &Args, cancel: &CancelToken) -> crate_core::Result<ExitCode> {
    let name = args.archive.to_string_lossy();
    let Some((format, age)) = OutFormat::from_file_name(&name) else {
        eprintln!("crate: {name}: can't tell the format from the name (use .zip, .7z, .tar.zst…)");
        return Ok(ExitCode::from(2));
    };
    if age && args.password.is_none() {
        eprintln!("crate: {name}: .age archives need a password (-p)");
        return Ok(ExitCode::from(2));
    }
    if args.archive.exists() && !args.overwrite {
        eprintln!("crate: {name} already exists (use --overwrite to replace it)");
        return Ok(ExitCode::FAILURE);
    }
    let (min, max, default) = format.levels();
    let options = CreateOptions {
        format,
        level: args.level.unwrap_or(default).clamp(min, max),
        password: args.password.clone(),
        exclude: args.exclude.clone(),
        keep_permissions: args.keep_permissions,
        volume_size: args.volume,
    };
    let sources: Vec<PathBuf> = args.rest.iter().map(PathBuf::from).collect();
    let collected = create::collect(
        &sources,
        &options.exclude,
        options.keep_permissions,
        Some(&args.archive),
    );
    for (path, why) in &collected.unreadable {
        eprintln!("crate: {}: {why} (left out)", path.display());
    }
    let report = create::create(
        &collected,
        &args.archive,
        &options,
        &mut show_progress,
        cancel,
    )?;
    clear_progress();
    let names: Vec<String> = report
        .outputs
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    println!(
        "Created {} ({} files, {} → {} bytes{})",
        names.join(", "),
        report.files,
        report.bytes,
        report.written,
        if report.excluded > 0 {
            format!(", {} excluded", report.excluded)
        } else {
            String::new()
        }
    );
    Ok(if collected.unreadable.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn edit_archive(args: &Args, cancel: &CancelToken) -> crate_core::Result<ExitCode> {
    let changes: Vec<Change> = match args.command.as_str() {
        "a" => vec![Change::Add {
            sources: args.rest.iter().map(PathBuf::from).collect(),
            into: args.into.clone(),
        }],
        "d" => args
            .rest
            .iter()
            .map(|p| Change::Delete(p.clone()))
            .collect(),
        _ => vec![Change::Rename {
            from: args.rest[0].clone(),
            to: args.rest[1].clone(),
        }],
    };
    let options = EditOptions {
        password: args.password.clone(),
        backup: args.backup,
        keep_permissions: args.keep_permissions,
        exclude: args.exclude.clone(),
    };
    let report = edit::apply(
        &args.archive,
        &changes,
        &options,
        &mut show_progress,
        cancel,
    )?;
    clear_progress();
    println!(
        "Updated {}: {} added, {} replaced, {} renamed, {} deleted{}",
        args.archive.display(),
        report.added,
        report.replaced,
        report.renamed,
        report.deleted,
        report
            .backup
            .map(|b| format!(" (previous version: {})", b.display()))
            .unwrap_or_default()
    );
    Ok(ExitCode::SUCCESS)
}

fn checksums(args: &Args, cancel: &CancelToken) -> crate_core::Result<ExitCode> {
    let c = checksum::compute(&args.archive, &mut |_| {}, cancel)?;
    let name = args.archive.display();
    println!(
        "SHA-256  {}  {name}\nBLAKE3   {}  {name}\nMD5      {}  {name}",
        c.sha256, c.blake3, c.md5
    );
    Ok(ExitCode::SUCCESS)
}

fn convert_archive(args: &Args, cancel: &CancelToken) -> crate_core::Result<ExitCode> {
    let out = PathBuf::from(&args.rest[0]);
    let name = out.to_string_lossy();
    let Some((format, age)) = OutFormat::from_file_name(&name) else {
        eprintln!("crate: {name}: can't tell the format from the name (use .zip, .7z, .tar.zst…)");
        return Ok(ExitCode::from(2));
    };
    if age && args.password.is_none() {
        eprintln!("crate: {name}: .age archives need a password (-p)");
        return Ok(ExitCode::from(2));
    }
    if out.exists() && !args.overwrite {
        eprintln!("crate: {name} already exists (use --overwrite to replace it)");
        return Ok(ExitCode::FAILURE);
    }
    let (min, max, default) = format.levels();
    let options = CreateOptions {
        format,
        level: args.level.unwrap_or(default).clamp(min, max),
        password: args.password.clone(),
        ..CreateOptions::default()
    };
    let report = convert::convert(
        &args.archive,
        args.source_password.as_deref(),
        &out,
        &options,
        &mut show_progress,
        cancel,
    )?;
    clear_progress();
    for s in &report.skipped {
        eprintln!("crate: {}: {} (left out)", s.path, s.reason);
    }
    println!(
        "Converted to {} ({} files)",
        out.display(),
        report.created.files
    );
    Ok(ExitCode::SUCCESS)
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
        .unwrap_or_else(|| crate_core::destination::folder_name(&args.archive).into());
    let mut options = ExtractOptions::default();
    if args.overwrite {
        options.if_exists = IfExists::Overwrite;
    }
    if args.allow_high_ratio {
        options.limits.max_ratio = None;
    }
    // Names this system won't take are renamed; say which afterwards.
    let names = archive.check_names(&Selection::All, &dest, &options);
    let report = archive.extract(&Selection::All, &dest, &options, &mut show_progress, cancel)?;
    clear_progress();
    for r in &names.renamed {
        eprintln!("  renamed {} → {} ({})", r.path, r.new_name, r.issue);
    }
    for c in &names.case_clashes {
        eprintln!(
            "  renamed {} → {} (same name as {} on this disk)",
            c.path, c.both_name, c.other
        );
    }
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
    fn time_format() {
        assert_eq!(
            format_time(UNIX_EPOCH + std::time::Duration::from_secs(1_792_845_015)),
            "2026-10-24 12:30"
        );
    }
}
