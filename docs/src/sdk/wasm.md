# WASM Plugin SDK

## Overview

WASM plugins run in a sandboxed WebAssembly environment via wasmtime.

## Template

Use the template at `plugins/wasm-sdk/`:

```bash
cd plugins/wasm-sdk
./build.sh
```

## Plugin Interface

```rust
/// Returns plugin metadata as JSON
#[no_mangle]
pub extern "C" fn plugin_info() -> *mut u8;

/// Execute plugin with JSON input
#[no_mangle]
pub extern "C" fn plugin_exec(input_ptr: *const u8, input_len: usize) -> *mut u8;

/// Free allocated memory
#[no_mangle]
pub extern "C" fn plugin_free(ptr: *mut u8, len: usize);
```

## Plugin JSON

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "runtime": "wasmtime",
  "entry": "my_plugin.wasm",
  "capabilities": ["exec"],
  "permissions": []
}
```

## Installation

```bash
cp my_plugin.wasm ~/.lingshu/plugins/my-plugin/
```
