#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    // Fuzz credential JSON structures — encryption input payloads
    if let Ok(s) = std::str::from_utf8(data) {
        // Try parsing as credential JSON
        if let Ok(val) = serde_json::from_str::<Value>(s) {
            // Common credential fields that must not panic on arbitrary input
            let _ = val.get("encrypted_token").and_then(|v| v.as_str());
            let _ = val.get("nonce").and_then(|v| v.as_str());
            let _ = val.get("provider").and_then(|v| v.as_str());
            let _ = val.get("credential_type").and_then(|v| v.as_str());
            let _ = val.get("scopes").and_then(|v| v.as_str());
        }

        // Fuzz AES-GCM nonce + ciphertext parsing
        if let Ok(val) = serde_json::from_str::<Value>(s) {
            if let (Some(nonce), Some(ct)) = (
                val.get("nonce").and_then(|v| v.as_str()),
                val.get("ciphertext").and_then(|v| v.as_str()),
            ) {
                // Try hex decode — must not panic
                let _ = hex::decode(nonce);
                let _ = hex::decode(ct);
            }
        }
    }
});
