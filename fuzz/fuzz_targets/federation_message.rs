#![no_main]

use libfuzzer_sys::fuzz_target;
use lingshu_federation::FederationMessage;

fuzz_target!(|data: &[u8]| {
    // Fuzz FederationMessage deserialization from arbitrary bytes
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<FederationMessage>(s);
    }
});
