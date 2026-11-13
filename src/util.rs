use pumpkin_plugin_api::{
    Player,
    command::{Arg, CommandError, CommandSender, ConsumedArgs},
    text::TextComponent,
};

/// A block coordinate (x, y, z).
pub type P = (i32, i32, i32);

pub fn err(message: &str) -> CommandError {
    CommandError::CommandFailed(TextComponent::text(message))
}

pub fn say(sender: &CommandSender, message: &str) {
    sender.send_message(TextComponent::text(message));
}

pub fn player_of(sender: &CommandSender) -> Result<Player, CommandError> {
    sender
        .as_player()
        .ok_or_else(|| err("Only a player can use this command."))
}

/// Reads a text-like argument (single word, greedy text, or a block/state argument).
pub fn arg_text(args: &ConsumedArgs, key: &str) -> Result<String, CommandError> {
    match args.get_value(key) {
        Arg::Simple(value)
        | Arg::Msg(value)
        | Arg::Block(value)
        | Arg::BlockPredicate(value) => Ok(value),
        _ => Err(err("A text argument was expected.")),
    }
}

/// Iterates every position in the inclusive cuboid between `min` and `max`.
pub fn cuboid(min: P, max: P) -> impl Iterator<Item = P> {
    (min.0..=max.0).flat_map(move |x| {
        (min.1..=max.1).flat_map(move |y| (min.2..=max.2).map(move |z| (x, y, z)))
    })
}

pub fn volume(min: P, max: P) -> u64 {
    let dx = (max.0 as i64 - min.0 as i64 + 1).max(0) as u64;
    let dy = (max.1 as i64 - min.1 as i64 + 1).max(0) as u64;
    let dz = (max.2 as i64 - min.2 as i64 + 1).max(0) as u64;
    dx * dy * dz
}

/// The block the player is standing in.
pub fn feet_block(player: &Player) -> P {
    let (x, y, z) = player.get_position();
    (x.floor() as i32, y.floor() as i32, z.floor() as i32)
}

pub fn player_key(player: &Player) -> String {
    player.get_id().to_string()
}
