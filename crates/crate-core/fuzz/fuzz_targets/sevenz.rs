#![no_main]
//! The sevenz reader on arbitrary bytes (seed corpus: corpus/sevenz/).

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    crate_core_fuzz::exercise("archive.input", data);
});
