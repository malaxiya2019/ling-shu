# Plugin Marketplace

## Available Plugins

| Plugin | Runtime | Description |
|--------|---------|-------------|
| beef-plugin | native | BEEF protocol integration |
| watch-plugin | native | File system watching |
| wasm-template | wasmtime | WASM plugin template |

## Installation

```bash
# Via marketplace install script
bash plugins/marketplace/install.sh beef-plugin

# Manual
cp plugin.wasm ~/.lingshu/plugins/plugin-name/
```

## Creating a Marketplace Entry

Add to `plugins/marketplace/index.json`:

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "description": "My custom plugin",
  "runtime": "wasmtime",
  "entry": "my_plugin.wasm",
  "capabilities": ["exec"],
  "permissions": []
}
```
