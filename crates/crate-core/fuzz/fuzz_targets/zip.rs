#![no_main]
//! The zip reader on arbitrary bytes (seed corpus: corpus/zip/).

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    crate_core_fuzz::exercise("archive.input", data);
});
