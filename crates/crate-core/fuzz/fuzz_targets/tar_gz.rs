#![no_main]
//! The tar.gz reader on arbitrary bytes (seed corpus: corpus/tar_gz/).

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    crate_core_fuzz::exercise("archive.input", data);
});
