use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, OnceLock},
};

use pumpkin_plugin_api::Player;

use crate::util::{P, player_key};

#[derive(Clone, Default)]
pub struct Selection {
    pub world: String,
    pub pos1: Option<P>,
    pub pos2: Option<P>,
}

#[derive(Clone)]
pub struct Clipboard {
    /// (dx, dy, dz, state id) relative to where the player stood when copying.
    pub blocks: Vec<(i32, i32, i32, u16)>,
}

pub struct UndoEntry {
    pub world: String,
    /// (position, previous state id)
    pub blocks: Vec<(P, u16)>,
}

#[derive(Default)]
pub struct Sessions {
    pub selections: HashMap<String, Selection>,
    pub clipboards: HashMap<String, Clipboard>,
    pub undo: HashMap<String, Vec<UndoEntry>>,
}

static SESSIONS: OnceLock<Mutex<Sessions>> = OnceLock::new();

pub fn sessions() -> MutexGuard<'static, Sessions> {
    SESSIONS
        .get_or_init(|| Mutex::new(Sessions::default()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// Sets point 1 or 2 for the player. Returns `false` if it was already set to that position.
pub fn set_point(player: &Player, which: u8, pos: P) -> bool {
    let world = player.get_world().get_name();
    let mut sessions = sessions();
    let selection = sessions.selections.entry(player_key(player)).or_default();
    if selection.world != world {
        *selection = Selection {
            world,
            pos1: None,
            pos2: None,
        };
    }
    let slot = if which == 1 {
        &mut selection.pos1
    } else {
        &mut selection.pos2
    };
    if *slot == Some(pos) {
        return false;
    }
    *slot = Some(pos);
    true
}

pub fn clear(key: &str) {
    sessions().selections.remove(key);
}

/// Returns (world, min corner, max corner) when both points are set.
pub fn bounds(key: &str) -> Option<(String, P, P)> {
    let sessions = sessions();
    let selection = sessions.selections.get(key)?;
    let (a, b) = (selection.pos1?, selection.pos2?);
    let min = (a.0.min(b.0), a.1.min(b.1), a.2.min(b.2));
    let max = (a.0.max(b.0), a.1.max(b.1), a.2.max(b.2));
    Some((selection.world.clone(), min, max))
}
