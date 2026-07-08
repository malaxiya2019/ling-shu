# Plugins

LingShu supports two types of plugins:

## Native Plugins
- Shared libraries (.so/.dylib/.dll)
- High performance
- Direct access to system APIs

## WASM Plugins
- WebAssembly sandbox execution
- Platform independent
- Isolated from host system
- Resource limited

## Plugin Lifecycle

1. **Discovery**: Scan plugin directories
2. **Load**: Load metadata from `plugin.json`
3. **Initialize**: Call `plugin_init()` (native) or load WASM module
4. **Execute**: Handle incoming requests
5. **Shutdown**: Clean up resources

## Creating a Plugin

See [WASM Plugin SDK](sdk/wasm.md) and [Plugin Marketplace](sdk/marketplace.md) for detailed guides.
