use std::time::SystemTime;

/// Archive formats Crate can open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    TarBz2,
    SevenZ,
    Rar,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Zip => "ZIP",
            Format::Tar => "TAR",
            Format::TarGz => "TAR.GZ",
            Format::TarXz => "TAR.XZ",
            Format::TarZst => "TAR.ZST",
            Format::TarBz2 => "TAR.BZ2",
            Format::SevenZ => "7Z",
            Format::Rar => "RAR",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink {
        target: String,
    },
    /// A hard link to an earlier entry (tar).
    Hardlink {
        target: String,
    },
    /// Devices, FIFOs and anything else that is never extracted.
    Other,
}

/// One item in an archive, as listed. `path` is the decoded name exactly as
/// stored; it may be unsafe; `crate::safety` decides where (and whether) it
/// is written.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Position in the archive; stable for the life of an [`crate::Archive`].
    pub index: usize,
    pub path: String,
    pub kind: EntryKind,
    /// Unpacked size in bytes, as declared by the archive.
    pub size: u64,
    /// Packed size, when the format stores it per entry.
    pub packed_size: Option<u64>,
    pub modified: Option<SystemTime>,
    /// Unix permission bits, when stored.
    pub mode: Option<u32>,
    pub crc32: Option<u32>,
    pub encrypted: bool,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }
}
