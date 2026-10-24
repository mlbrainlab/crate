//! Writes the generated test archives into `tests/fixtures/`, for trying the
//! CLI by hand: `cargo run -p crate-core --example gen_fixtures`.
//! The tests build their own copies; these files are a convenience.

#[path = "../tests/support/mod.rs"]
mod support;

use std::fs;
use std::path::Path;

type Generator = fn(&Path);

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let all: &[(&str, Generator)] = &[
        ("sample.zip", support::sample_zip),
        ("arabic-cp1256.zip", support::arabic_cp1256_zip),
        ("encrypted/aes256.zip", support::encrypted_zip),
        ("damaged/bad-crc.zip", support::bad_crc_zip),
        ("malicious/traversal.zip", support::traversal_zip),
        ("malicious/symlinks.zip", support::symlink_zip),
        ("malicious/lying-size.zip", support::lying_size_zip),
        ("malicious/bomb.zip", support::bomb_zip),
    ];
    for (name, make) in all {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        make(&path);
        println!("{}", path.canonicalize().unwrap().display());
    }
}
