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

## WorldEdit — permission `worldtools:edit` (op level 2)

Use the classic double-slash forms (`//set`) or the `/we ...` equivalents (`/we set`).

| Command | What it does |
| --- | --- |
| Wooden axe | Left-click a block = position 1, right-click = position 2 |
| `//pos1`, `//pos2` | Set a position at your feet |
| `//size`, `//sel` (`//desel`), `//wand` | Show / clear the selection, wand help |
| `//set <pattern>` | Fill the selection with a pattern |
| `//replace <from> <to>` (`//re`) | Replace the `from` blocks with a pattern |
| `//replace <to>` | Replace every non-air block with a pattern |
| `//copy`, `//paste` | Copy relative to where you stand, paste where you stand |
| `//undo` | Undo your last edit (20 steps kept) |

**Patterns**

```text
//set stone                          one block
//set 0                              legacy numeric ids work (0 = air, 1 = stone, 5 = oak_planks, ...)
//set stone,dirt                     equal chance of each
//set 50%stone,50%stone_bricks       weighted random mix
//set 70%grass_block,30%dirt
//replace stone,dirt air             from can be a list of blocks
//replace 0 glass                    numeric ids work in both places
```

Random patterns are decided per position, so the same selection filled twice gets a fresh mix.
Block state properties (`oak_stairs[facing=north]`) are accepted but ignored; the block's default
state is used. Only a handful of the old numeric ids are mapped (0-5, 7-18, 20, 45, 49); use block
names for everything else. Limit: 250,000 blocks per operation. Paste copies block states only
(no containers or NBT).

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
- No region priorities, no per-flag groups, no redo, no rotation or flipping on paste, no `//pos` by coordinates.
- Y range is not clamped; keep selections inside the world's build height.
- Right-click wand events may fire once per hand depending on the server version; repeated
  identical positions are not re-announced.
