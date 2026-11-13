//! Basic WorldEdit-style commands: selection, set, replace, copy, paste, undo.

use pumpkin_plugin_api::{
    Player, Server, World,
    command::{
        ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs, StringType,
    },
    commands::CommandHandler,
    common::BlockPos,
    world::BlockFlags,
};

use crate::{
    pattern::{self, Mask, Pattern, lookup_block},
    selection::{self, Clipboard, UndoEntry},
    util::{P, arg_text, cuboid, err, feet_block, player_key, player_of, say, volume},
};

/// Largest number of blocks a single operation may touch.
pub const MAX_BLOCKS: u64 = 250_000;
/// How many undo steps are kept per player.
const MAX_UNDO: usize = 20;

pub const WAND_ITEM: &str = "minecraft:wooden_axe";

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

/// Every command this module provides: the `/we ...` tree plus the classic `//...` forms.
pub fn commands() -> Vec<Command> {
    let word = ArgumentType::String(StringType::SingleWord);
    let greedy = ArgumentType::String(StringType::Greedy);

    let we = Command::new(&names(&["we", "worldedit"]), "Basic WorldEdit commands")
        .then(CommandNode::literal("pos1").execute(SetPoint(1)))
        .then(CommandNode::literal("pos2").execute(SetPoint(2)))
        .then(CommandNode::literal("wand").execute(Wand))
        .then(CommandNode::literal("clear").execute(ClearSelection))
        .then(CommandNode::literal("size").execute(Size))
        .then(
            CommandNode::literal("set")
                .then(CommandNode::argument("pattern", &greedy).execute(SetBlocks)),
        )
        .then(
            CommandNode::literal("replace").then(
                CommandNode::argument("from", &word)
                    .execute(ReplaceAll)
                    .then(CommandNode::argument("to", &greedy).execute(ReplaceBlocks)),
            ),
        )
        .then(CommandNode::literal("copy").execute(Copy))
        .then(CommandNode::literal("paste").execute(Paste))
        .then(CommandNode::literal("undo").execute(Undo));

    vec![
        we,
        Command::new(&names(&["/pos1"]), "Set position 1 at your feet").execute(SetPoint(1)),
        Command::new(&names(&["/pos2"]), "Set position 2 at your feet").execute(SetPoint(2)),
        Command::new(&names(&["/wand"]), "How to use the selection wand").execute(Wand),
        Command::new(&names(&["/sel", "/desel"]), "Clear your selection")
            .execute(ClearSelection),
        Command::new(&names(&["/size"]), "Show your selection size").execute(Size),
        Command::new(&names(&["/set"]), "Fill the selection with a block pattern")
            .then(CommandNode::argument("pattern", &greedy).execute(SetBlocks)),
        Command::new(&names(&["/replace", "/re"]), "Replace blocks in the selection").then(
            CommandNode::argument("from", &word)
                .execute(ReplaceAll)
                .then(CommandNode::argument("to", &greedy).execute(ReplaceBlocks)),
        ),
        Command::new(&names(&["/copy"]), "Copy the selection").execute(Copy),
        Command::new(&names(&["/paste"]), "Paste your clipboard at your feet").execute(Paste),
        Command::new(&names(&["/undo"]), "Undo your last edit").execute(Undo),
    ]
}

/// The player's validated selection: (world name, min, max).
fn selection_for(player: &Player) -> Result<(String, P, P), CommandError> {
    let (world, min, max) = selection::bounds(&player_key(player))
        .ok_or_else(|| err("Select two positions first (wooden axe, /we pos1, /we pos2)."))?;
    if world != player.get_world().get_name() {
        return Err(err("Your selection is in a different world."));
    }
    let count = volume(min, max);
    if count > MAX_BLOCKS {
        return Err(err(&format!(
            "Selection too large ({count} blocks, limit {MAX_BLOCKS})."
        )));
    }
    Ok((world, min, max))
}

fn block_pos(p: P) -> BlockPos {
    BlockPos {
        x: p.0,
        y: p.1,
        z: p.2,
    }
}

/// Sets blocks and records the previous states as one undo step. Returns blocks changed.
fn apply(
    key: &str,
    world: &World,
    world_name: &str,
    changes: impl Iterator<Item = (P, u16)>,
) -> usize {
    let mut previous: Vec<(P, u16)> = Vec::new();
    for (pos, new_state) in changes {
        let old_state = world.get_block_state_id(block_pos(pos));
        if old_state == new_state {
            continue;
        }
        previous.push((pos, old_state));
        world.set_block_state(block_pos(pos), new_state, BlockFlags::NOTIFY_LISTENERS);
    }
    let changed = previous.len();
    if changed > 0 {
        let mut sessions = selection::sessions();
        let stack = sessions.undo.entry(key.to_string()).or_default();
        stack.push(UndoEntry {
            world: world_name.to_string(),
            blocks: previous,
        });
        if stack.len() > MAX_UNDO {
            stack.remove(0);
        }
    }
    changed
}

struct SetPoint(u8);
impl CommandHandler for SetPoint {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let pos = feet_block(&player);
        selection::set_point(&player, self.0, pos);
        say(
            &sender,
            &format!("Position {} set to ({}, {}, {}).", self.0, pos.0, pos.1, pos.2),
        );
        Ok(1)
    }
}

struct Wand;
impl CommandHandler for Wand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        say(
            &sender,
            "Hold a wooden axe: left-click a block for position 1, right-click for position 2.",
        );
        Ok(1)
    }
}

struct ClearSelection;
impl CommandHandler for ClearSelection {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        selection::clear(&player_key(&player));
        say(&sender, "Selection cleared.");
        Ok(1)
    }
}

struct Size;
impl CommandHandler for Size {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let (_, min, max) = selection_for(&player)?;
        say(
            &sender,
            &format!(
                "Selection: ({}, {}, {}) to ({}, {}, {}), {} blocks.",
                min.0, min.1, min.2, max.0, max.1, max.2, volume(min, max)
            ),
        );
        Ok(1)
    }
}

struct SetBlocks;
impl CommandHandler for SetBlocks {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let (world_name, min, max) = selection_for(&player)?;
        let pattern = Pattern::parse(&arg_text(&args, "pattern")?)?;
        let salt = pattern::next_salt();
        let world = player.get_world();
        let changed = apply(
            &player_key(&player),
            &world,
            &world_name,
            cuboid(min, max).map(|pos| (pos, pattern.pick(pos, salt))),
        );
        say(&sender, &format!("{changed} block(s) changed."));
        Ok(changed as i32)
    }
}

struct ReplaceBlocks;
impl CommandHandler for ReplaceBlocks {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let (world_name, min, max) = selection_for(&player)?;
        let mask = Mask::parse(&arg_text(&args, "from")?)?;
        let pattern = Pattern::parse(&arg_text(&args, "to")?)?;
        replace_matching(&sender, &player, &world_name, min, max, |id| mask.matches(id), &pattern)
    }
}

/// `//replace <to>`: replaces every non-air block in the selection.
struct ReplaceAll;
impl CommandHandler for ReplaceAll {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let (world_name, min, max) = selection_for(&player)?;
        let pattern = Pattern::parse(&arg_text(&args, "from")?)?;
        let air = lookup_block("air")?.id;
        replace_matching(&sender, &player, &world_name, min, max, |id| id != air, &pattern)
    }
}

fn replace_matching(
    sender: &CommandSender,
    player: &Player,
    world_name: &str,
    min: P,
    max: P,
    matches: impl Fn(u16) -> bool,
    pattern: &Pattern,
) -> Result<i32, CommandError> {
    let salt = pattern::next_salt();
    let world = player.get_world();
    let targets: Vec<P> = cuboid(min, max)
        .filter(|pos| matches(world.get_block(block_pos(*pos)).id))
        .collect();
    let changed = apply(
        &player_key(player),
        &world,
        world_name,
        targets.into_iter().map(|pos| (pos, pattern.pick(pos, salt))),
    );
    say(sender, &format!("{changed} block(s) replaced."));
    Ok(changed as i32)
}

struct Copy;
impl CommandHandler for Copy {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let (_, min, max) = selection_for(&player)?;
        let origin = feet_block(&player);
        let world = player.get_world();
        let blocks: Vec<(i32, i32, i32, u16)> = cuboid(min, max)
            .map(|pos| {
                (
                    pos.0 - origin.0,
                    pos.1 - origin.1,
                    pos.2 - origin.2,
                    world.get_block_state_id(block_pos(pos)),
                )
            })
            .collect();
        let count = blocks.len();
        selection::sessions()
            .clipboards
            .insert(player_key(&player), Clipboard { blocks });
        say(&sender, &format!("Copied {count} block(s). Stand where you want it and use /we paste."));
        Ok(count as i32)
    }
}

struct Paste;
impl CommandHandler for Paste {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let key = player_key(&player);
        let clipboard = selection::sessions()
            .clipboards
            .get(&key)
            .cloned()
            .ok_or_else(|| err("Your clipboard is empty. Use /we copy first."))?;
        let origin = feet_block(&player);
        let world = player.get_world();
        let world_name = world.get_name();
        let changed = apply(
            &key,
            &world,
            &world_name,
            clipboard.blocks.iter().map(|(dx, dy, dz, state)| {
                ((origin.0 + dx, origin.1 + dy, origin.2 + dz), *state)
            }),
        );
        say(&sender, &format!("Pasted, {changed} block(s) changed."));
        Ok(changed as i32)
    }
}

struct Undo;
impl CommandHandler for Undo {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let entry = selection::sessions()
            .undo
            .get_mut(&player_key(&player))
            .and_then(|stack| stack.pop())
            .ok_or_else(|| err("Nothing to undo."))?;
        let world = server
            .get_world_by_name(&entry.world)
            .ok_or_else(|| err("The world for that edit is no longer loaded."))?;
        // Restore in reverse so overlapping writes end up at the oldest state.
        for (pos, state) in entry.blocks.iter().rev() {
            world.set_block_state(block_pos(*pos), *state, BlockFlags::NOTIFY_LISTENERS);
        }
        say(&sender, &format!("Undid {} block(s).", entry.blocks.len()));
        Ok(entry.blocks.len() as i32)
    }
}
