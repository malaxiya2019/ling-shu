with open('Cargo.toml', 'r') as f:
    content = f.read()

# Remove the incorrectly placed hashbrown line outside [patch.crates-io]
old = '''[patch.crates-io]
sqlx-sqlite = { path = "crates/sqlx-sqlite-fork" }

# Patch hashbrown to 0.16.1 for starlark_map + allocative compatibility
hashbrown = "0.16.1"'''

new = '''[patch.crates-io]
sqlx-sqlite = { path = "crates/sqlx-sqlite-fork" }
hashbrown = "0.16.1"'''

if old in content:
    content = content.replace(old, new)
    with open('Cargo.toml', 'w') as f:
        f.write(content)
    print('Fixed patch section')
else:
    print('Could not find the pattern')
    # Show the tail
    lines = content.split('\n')
    for l in lines[-8:]:
        print(repr(l))
