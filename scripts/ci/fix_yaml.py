import re

with open('.github/workflows/ci.yml', 'r') as f:
    content = f.read()

# Fix plugin loading test: replace the python3 -c inline block
old_block = '''          curl -sf http://127.0.0.1:8091/plugins | python3 -c "
import json, sys
data = json.load(sys.stdin)
print(f'Loaded plugins: {len(data)}')
for p in data:
    print(f'  - {p.get(\"name\", \"?\")} v{p.get(\"version\", \"?\")}')
" 2>/dev/null || echo "plugin API check skipped"'''

new_block = '''          curl -sf http://127.0.0.1:8091/plugins 2>/dev/null | python3 scripts/ci/test_plugin_loading.py || echo "plugin API check skipped"'''

if old_block in content:
    content = content.replace(old_block, new_block)
    print("Fixed plugin loading block")
else:
    print("Could not find plugin loading block")
    # Find the block differently
    idx = content.find('plugins | python3')
    if idx >= 0:
        end_idx = content.find('\n', idx + 200)
        if end_idx < 0:
            end_idx = idx + 300
        print("Found at index", idx)
        print(repr(content[idx-20:end_idx]))

with open('.github/workflows/ci.yml', 'w') as f:
    f.write(content)

# Validate
try:
    import yaml
    yaml.safe_load(content)
    print("YAML validation: OK")
except Exception as e:
    print(f"YAML validation: {e}")
