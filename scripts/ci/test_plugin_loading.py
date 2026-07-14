import json, sys
data = json.load(sys.stdin)
print(f'Loaded plugins: {len(data)}')
for p in data:
    print(f'  - {p.get("name", "?")} v{p.get("version", "?")}')
