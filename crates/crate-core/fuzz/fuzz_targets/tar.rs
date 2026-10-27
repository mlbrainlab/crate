#![no_main]
//! The tar reader on arbitrary bytes (seed corpus: corpus/tar/).

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    crate_core_fuzz::exercise("archive.input", data);
});
