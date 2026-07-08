#![no_main]

use libfuzzer_sys::fuzz_target;
use lingshu_mcp::JsonRpcRequest;
use serde::Serialize;
use serde_json::Value;

fuzz_target!(|data: &[u8]| {
    // Fuzz MCP JSON-RPC method call payloads
    if let Ok(s) = std::str::from_utf8(data) {
        // Try as a generic JSON-RPC request with Value params
        let _ = serde_json::from_str::<JsonRpcRequest<Value>>(s);

        // Try as a tools/call request with Value params
        if let Ok(req) = serde_json::from_str::<JsonRpcRequest<Value>>(s) {
            let _ = req.method.as_str();
        }
    }
});
