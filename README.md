# worldtools

A basic WorldEdit + WorldGuard style plugin for [Pumpkin](https://pumpkinmc.org), written in Rust against
`pumpkin-plugin-api`. Structure and API usage follow `theeagler1234/pumpkin-NPCs` and the official
Rust plugin docs (<https://docs.pumpkinmc.org/plugin-dev/rust>).

## Build

```sh
rustup target add wasm32-wasip2
cargo build --release
```

Copy `target/wasm32-wasip2/release/pumpkin_worldtools.wasm` into your server's `plugins/` folder.

## WorldEdit (`/we`, alias `/worldedit`) — permission `worldtools:edit` (op level 2)

| Command | What it does |
| --- | --- |
| Wooden axe | Left-click a block = position 1, right-click = position 2 |
| `/we pos1`, `/we pos2` | Set a position at your feet |
| `/we size`, `/we clear`, `/we wand` | Show / clear the selection, wand help |
| `/we set <block>` | Fill the selection |
| `/we replace <from> <to>` | Replace one block type with another |
| `/we copy`, `/we paste` | Copy relative to where you stand, paste where you stand |
| `/we undo` | Undo your last edit (20 steps kept) |

Limits: 250,000 blocks per operation. Block state properties (`[facing=north]`) are ignored; the
block's default state is used. Paste copies block states only (no containers/NBT).

## WorldGuard (`/wg`, alias `/region`) — permission `worldtools:guard` (op level 3)

| Command | What it does |
| --- | --- |
| `/wg define <name>` | Create a region from your selection; you become owner |
| `/wg remove <name>`, `/wg list`, `/wg info <name>` | Manage and inspect regions |
| `/wg flag <name> <build\|interact\|pvp> <allow\|deny\|none>` | Set a flag |
| `/wg addmember <name> <player>`, `/wg removemember ...` | Membership (player must be online) |

Flag defaults: `build` is denied to non-members, `interact` and `pvp` are allowed. Overlapping
regions: the smallest one at a position wins. Players with `worldtools:bypass` (op level 3)
ignore protection. Regions persist to `regions.json` in the plugin data folder.

## Known limitations

- Explosion, fire, mob-griefing and fluid-flow protection are not included: the matching events
  carry no world information in the current API, so they can't be matched to a region safely.
- No region priorities, no per-flag groups, no redo, no rotation or flipping on paste.
- Y range is not clamped; keep selections inside the world's build height.
- Right-click wand events may fire once per hand depending on the server version; repeated
  identical positions are not re-announced.
