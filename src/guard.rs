//! Basic WorldGuard-style regions: define, flag, membership, and protection handlers.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, OnceLock},
};

use pumpkin_plugin_api::{
    Player, Server,
    command::{ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs, StringType},
    commands::CommandHandler,
    events::{
        BlockBreakEvent, BlockPlaceEvent, EntityDamageByEntityEvent, EventData, EventHandler,
        InteractAction, PlayerInteractEvent,
    },
    text::TextComponent,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    edit::WAND_ITEM,
    selection,
    util::{P, arg_text, err, player_key, player_of, say, volume},
};

pub const BYPASS_PERMISSION: &str = "worldtools:bypass";
pub const EDIT_PERMISSION: &str = "worldtools:edit";

#[derive(Serialize, Deserialize, Clone)]
pub struct Region {
    pub name: String,
    pub world: String,
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub owners: Vec<String>,
    pub members: Vec<String>,
    /// flag name -> true (allow) / false (deny). Unset flags use their default.
    pub flags: HashMap<String, bool>,
}

#[derive(Clone, Copy)]
pub enum Flag {
    /// Breaking and placing blocks. Default: denied for non-members.
    Build,
    /// Right-clicking blocks (doors, chests...). Default: allowed.
    Interact,
    /// Player vs player damage. Default: allowed.
    Pvp,
}

impl Flag {
    pub fn name(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Interact => "interact",
            Self::Pvp => "pvp",
        }
    }

    fn default_allow(self) -> bool {
        !matches!(self, Self::Build)
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "build" => Some(Self::Build),
            "interact" => Some(Self::Interact),
            "pvp" => Some(Self::Pvp),
            _ => None,
        }
    }
}

static REGIONS: Mutex<Vec<Region>> = Mutex::new(Vec::new());
static DATA_FOLDER: OnceLock<String> = OnceLock::new();

fn regions() -> MutexGuard<'static, Vec<Region>> {
    REGIONS.lock().unwrap_or_else(|error| error.into_inner())
}

fn data_file() -> Option<String> {
    DATA_FOLDER.get().map(|folder| format!("{folder}/regions.json"))
}

pub fn load(data_folder: String) {
    let _ = DATA_FOLDER.set(data_folder);
    let Some(path) = data_file() else { return };
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Vec<Region>>(&text) {
            Ok(loaded) => *regions() = loaded,
            Err(error) => warn!("Could not parse {path}: {error}"),
        },
        Err(_) => {} // First run: nothing saved yet.
    }
}

pub fn save() {
    let Some(path) = data_file() else { return };
    let text = match serde_json::to_string_pretty(&*regions()) {
        Ok(text) => text,
        Err(error) => {
            warn!("Could not serialize regions: {error}");
            return;
        }
    };
    if let Err(error) = std::fs::write(&path, text) {
        warn!("Could not write {path}: {error}");
    }
}

fn region_volume(region: &Region) -> u64 {
    volume(
        (region.min[0], region.min[1], region.min[2]),
        (region.max[0], region.max[1], region.max[2]),
    )
}

/// The most specific (smallest) region containing the position.
fn region_at<'a>(list: &'a [Region], world: &str, pos: P) -> Option<&'a Region> {
    list.iter()
        .filter(|region| {
            region.world == world
                && pos.0 >= region.min[0]
                && pos.0 <= region.max[0]
                && pos.1 >= region.min[1]
                && pos.1 <= region.max[1]
                && pos.2 >= region.min[2]
                && pos.2 <= region.max[2]
        })
        .min_by_key(|region| region_volume(region))
}

/// Whether the player may perform an action governed by `flag` at `pos`.
pub fn allowed(player: &Player, flag: Flag, world: &str, pos: P) -> bool {
    if player.has_permission(BYPASS_PERMISSION) {
        return true;
    }
    let list = regions();
    let Some(region) = region_at(&list, world, pos) else {
        return true;
    };
    let id = player_key(player);
    if region.owners.contains(&id) || region.members.contains(&id) {
        return true;
    }
    region
        .flags
        .get(flag.name())
        .copied()
        .unwrap_or_else(|| flag.default_allow())
}

fn deny_message(player: &Player, what: &str) {
    player.send_system_message(TextComponent::text(&format!("You can't {what} here.")), true);
}

// ---------------------------------------------------------------- events

pub struct BreakGuard;
impl EventHandler<BlockBreakEvent> for BreakGuard {
    fn handle(
        &self,
        _server: Server,
        mut event: EventData<BlockBreakEvent>,
    ) -> EventData<BlockBreakEvent> {
        if let Some(player) = event.player.as_ref() {
            let pos = (event.block_pos.x, event.block_pos.y, event.block_pos.z);
            if !allowed(player, Flag::Build, &player.get_world().get_name(), pos) {
                deny_message(player, "break blocks");
                event.cancelled = true;
            }
        }
        event
    }
}

pub struct PlaceGuard;
impl EventHandler<BlockPlaceEvent> for PlaceGuard {
    fn handle(
        &self,
        _server: Server,
        mut event: EventData<BlockPlaceEvent>,
    ) -> EventData<BlockPlaceEvent> {
        let pos = (event.block_pos.x, event.block_pos.y, event.block_pos.z);
        if !allowed(&event.player, Flag::Build, &event.player.get_world().get_name(), pos) {
            deny_message(&event.player, "place blocks");
            event.cancelled = true;
        }
        event
    }
}

/// Handles the selection wand (WorldEdit) and the `interact` flag (WorldGuard).
pub struct InteractGuard;
impl EventHandler<PlayerInteractEvent> for InteractGuard {
    fn handle(
        &self,
        _server: Server,
        mut event: EventData<PlayerInteractEvent>,
    ) -> EventData<PlayerInteractEvent> {
        let left = matches!(event.action, InteractAction::LeftClickBlock);
        let right = matches!(event.action, InteractAction::RightClickBlock);
        let Some(clicked) = event.clicked_pos.as_ref().map(|p| (p.x, p.y, p.z)) else {
            return event;
        };
        if !left && !right {
            return event;
        }

        let holding_wand = event
            .player
            .get_item_in_hand(pumpkin_plugin_api::common::Hand::Right)
            .is_some_and(|stack| stack.get_registry_key() == WAND_ITEM);
        if holding_wand && event.player.has_permission(EDIT_PERMISSION) {
            let which = if left { 1 } else { 2 };
            if selection::set_point(&event.player, which, clicked) {
                event.player.send_system_message(
                    TextComponent::text(&format!(
                        "Position {which} set to ({}, {}, {}).",
                        clicked.0, clicked.1, clicked.2
                    )),
                    false,
                );
            }
            event.cancelled = true;
            return event;
        }

        if right
            && !allowed(
                &event.player,
                Flag::Interact,
                &event.player.get_world().get_name(),
                clicked,
            )
        {
            deny_message(&event.player, "use that");
            event.cancelled = true;
        }
        event
    }
}

pub struct PvpGuard;
impl EventHandler<EntityDamageByEntityEvent> for PvpGuard {
    fn handle(
        &self,
        server: Server,
        mut event: EventData<EntityDamageByEntityEvent>,
    ) -> EventData<EntityDamageByEntityEvent> {
        let players = server.get_all_players();
        let find = |entity_id: i32| {
            players
                .iter()
                .find(|player| player.as_entity().get_id() as i32 == entity_id)
        };
        let (Some(victim), Some(_attacker)) = (find(event.entity_id), find(event.damager_id))
        else {
            return event;
        };
        let (x, y, z) = victim.get_position();
        let pos = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
        let world = victim.get_world().get_name();
        let list = regions();
        if let Some(region) = region_at(&list, &world, pos) {
            if region.flags.get("pvp").copied() == Some(false) {
                event.cancelled = true;
            }
        }
        event
    }
}

// -------------------------------------------------------------- commands

pub fn command() -> Command {
    let names = ["wg".to_string(), "region".to_string()];
    let word = ArgumentType::String(StringType::SingleWord);
    Command::new(&names, "Basic WorldGuard region commands")
        .then(
            CommandNode::literal("define")
                .then(CommandNode::argument("name", &word).execute(Define)),
        )
        .then(
            CommandNode::literal("remove")
                .then(CommandNode::argument("name", &word).execute(Remove)),
        )
        .then(CommandNode::literal("list").execute(List))
        .then(
            CommandNode::literal("info")
                .then(CommandNode::argument("name", &word).execute(Info)),
        )
        .then(
            CommandNode::literal("flag").then(
                CommandNode::argument("name", &word).then(
                    CommandNode::argument("flag", &word)
                        .then(CommandNode::argument("value", &word).execute(SetFlag)),
                ),
            ),
        )
        .then(
            CommandNode::literal("addmember").then(
                CommandNode::argument("name", &word)
                    .then(CommandNode::argument("player", &word).execute(Membership(true))),
            ),
        )
        .then(
            CommandNode::literal("removemember").then(
                CommandNode::argument("name", &word)
                    .then(CommandNode::argument("player", &word).execute(Membership(false))),
            ),
        )
}

fn with_region<T>(
    name: &str,
    action: impl FnOnce(&mut Region) -> T,
) -> Result<T, CommandError> {
    let mut list = regions();
    let region = list
        .iter_mut()
        .find(|region| region.name == name)
        .ok_or_else(|| err(&format!("No region named '{name}'.")))?;
    Ok(action(region))
}

struct Define;
impl CommandHandler for Define {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let player = player_of(&sender)?;
        let name = arg_text(&args, "name")?.to_lowercase();
        let (world, min, max) = selection::bounds(&player_key(&player))
            .ok_or_else(|| err("Select two positions first (see /we wand)."))?;
        {
            let mut list = regions();
            if list.iter().any(|region| region.name == name) {
                return Err(err("A region with that name already exists."));
            }
            list.push(Region {
                name: name.clone(),
                world,
                min: [min.0, min.1, min.2],
                max: [max.0, max.1, max.2],
                owners: vec![player_key(&player)],
                members: Vec::new(),
                flags: HashMap::new(),
            });
        }
        save();
        say(&sender, &format!("Region '{name}' defined ({} blocks).", volume(min, max)));
        Ok(1)
    }
}

struct Remove;
impl CommandHandler for Remove {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let name = arg_text(&args, "name")?.to_lowercase();
        {
            let mut list = regions();
            let before = list.len();
            list.retain(|region| region.name != name);
            if list.len() == before {
                return Err(err(&format!("No region named '{name}'.")));
            }
        }
        save();
        say(&sender, &format!("Region '{name}' removed."));
        Ok(1)
    }
}

struct List;
impl CommandHandler for List {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let names: Vec<String> = regions()
            .iter()
            .map(|region| format!("{} ({})", region.name, region.world))
            .collect();
        if names.is_empty() {
            say(&sender, "No regions defined.");
        } else {
            say(&sender, &format!("Regions: {}", names.join(", ")));
        }
        Ok(names.len() as i32)
    }
}

struct Info;
impl CommandHandler for Info {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let name = arg_text(&args, "name")?.to_lowercase();
        let lines = with_region(&name, |region| {
            let mut flags: Vec<String> = region
                .flags
                .iter()
                .map(|(flag, value)| format!("{flag}={}", if *value { "allow" } else { "deny" }))
                .collect();
            flags.sort();
            vec![
                format!("Region '{}' in {}", region.name, region.world),
                format!(
                    "Bounds: ({}, {}, {}) to ({}, {}, {})",
                    region.min[0], region.min[1], region.min[2], region.max[0], region.max[1],
                    region.max[2]
                ),
                format!("Owners: {}, members: {}", region.owners.len(), region.members.len()),
                format!("Flags: {}", if flags.is_empty() { "none".into() } else { flags.join(", ") }),
            ]
        })?;
        for line in lines {
            say(&sender, &line);
        }
        Ok(1)
    }
}

struct SetFlag;
impl CommandHandler for SetFlag {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let name = arg_text(&args, "name")?.to_lowercase();
        let flag_name = arg_text(&args, "flag")?.to_lowercase();
        let value = arg_text(&args, "value")?.to_lowercase();
        let flag = Flag::parse(&flag_name)
            .ok_or_else(|| err("Unknown flag. Available: build, interact, pvp."))?;
        let setting = match value.as_str() {
            "allow" => Some(true),
            "deny" => Some(false),
            "none" | "clear" => None,
            _ => return Err(err("Value must be allow, deny or none.")),
        };
        with_region(&name, |region| match setting {
            Some(value) => {
                region.flags.insert(flag.name().to_string(), value);
            }
            None => {
                region.flags.remove(flag.name());
            }
        })?;
        save();
        say(&sender, &format!("Flag '{}' on '{name}' set to {value}.", flag.name()));
        Ok(1)
    }
}

struct Membership(bool);
impl CommandHandler for Membership {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let name = arg_text(&args, "name")?.to_lowercase();
        let target_name = arg_text(&args, "player")?;
        let target = server
            .get_player_by_name(&target_name)
            .ok_or_else(|| err("That player must be online."))?;
        let id = player_key(&target);
        let add = self.0;
        with_region(&name, |region| {
            region.members.retain(|member| member != &id);
            if add {
                region.members.push(id.clone());
            }
        })?;
        save();
        say(
            &sender,
            &format!(
                "{} {target_name} {} region '{name}'.",
                if add { "Added" } else { "Removed" },
                if add { "to" } else { "from" }
            ),
        );
        Ok(1)
    }
}
