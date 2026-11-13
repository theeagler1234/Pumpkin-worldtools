//! WorldEdit-style block patterns (`stone`, `0`, `50%stone,50%stone_bricks`) and masks.

use std::sync::atomic::{AtomicU64, Ordering};

use pumpkin_plugin_api::{Block, BlockTypeExt, command::CommandError};

use crate::util::{P, err};

static OPERATION: AtomicU64 = AtomicU64::new(1);

/// A fresh value per operation so repeated random patterns don't all look the same.
pub fn next_salt() -> u64 {
    OPERATION.fetch_add(1, Ordering::Relaxed)
}

/// Splits on commas that are not inside `[...]` block property lists.
pub fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth <= 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Splits `50%stone` into `(50.0, "stone")`. Entries without a percentage weigh 1.
pub fn split_weight(entry: &str) -> Result<(f64, &str), String> {
    let Some((weight, block)) = entry.split_once('%') else {
        return Ok((1.0, entry));
    };
    let parsed: f64 = weight
        .trim()
        .parse()
        .map_err(|_| format!("'{}' is not a valid percentage.", weight.trim()))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err("Percentages must be greater than 0.".to_string());
    }
    let block = block.trim();
    if block.is_empty() {
        return Err("Missing a block name after '%'.".to_string());
    }
    Ok((parsed, block))
}

/// The classic numeric block ids people still type (`//set 0`).
fn legacy_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "0" => "air",
        "1" => "stone",
        "2" => "grass_block",
        "3" => "dirt",
        "4" => "cobblestone",
        "5" => "oak_planks",
        "7" => "bedrock",
        "8" | "9" => "water",
        "10" | "11" => "lava",
        "12" => "sand",
        "13" => "gravel",
        "14" => "gold_ore",
        "15" => "iron_ore",
        "16" => "coal_ore",
        "17" => "oak_log",
        "18" => "oak_leaves",
        "20" => "glass",
        "45" => "bricks",
        "49" => "obsidian",
        _ => return None,
    })
}

/// Looks a block up by name (`stone`, `minecraft:stone`, `stone[...]`) or legacy number.
/// Block state properties are ignored; the default state is used.
pub fn lookup_block(name: &str) -> Result<Block, CommandError> {
    let base = name.split('[').next().unwrap_or(name).trim();
    let base = legacy_name(base).unwrap_or(base);
    Block::of(base).ok_or_else(|| err(&format!("Unknown block '{base}'.")))
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Deterministic pseudo-random value in `[0, 1)` for a position.
fn unit_hash(pos: P, salt: u64) -> f64 {
    let hash = mix(
        salt ^ mix(pos.0 as i64 as u64)
            ^ mix(pos.1 as i64 as u64).rotate_left(21)
            ^ mix(pos.2 as i64 as u64).rotate_left(42),
    );
    (hash >> 11) as f64 / (1u64 << 53) as f64
}

/// A weighted set of block states to place.
pub struct Pattern {
    entries: Vec<(f64, u16)>,
    total: f64,
}

impl Pattern {
    pub fn from_entries(entries: Vec<(f64, u16)>) -> Self {
        let total = entries.iter().map(|(weight, _)| weight).sum();
        Self { entries, total }
    }

    pub fn parse(text: &str) -> Result<Self, CommandError> {
        let parts = split_top_level(text);
        if parts.is_empty() {
            return Err(err("Give a block or pattern, e.g. stone or 50%stone,50%dirt."));
        }
        let mut entries = Vec::new();
        for part in parts {
            let (weight, name) = split_weight(part).map_err(|message| err(&message))?;
            entries.push((weight, lookup_block(name)?.get_default_state().id));
        }
        Ok(Self::from_entries(entries))
    }

    pub fn pick(&self, pos: P, salt: u64) -> u16 {
        if self.entries.len() == 1 {
            return self.entries[0].1;
        }
        let target = unit_hash(pos, salt) * self.total;
        let mut running = 0.0;
        for (weight, state) in &self.entries {
            running += weight;
            if target < running {
                return *state;
            }
        }
        self.entries.last().map_or(0, |entry| entry.1)
    }
}

/// A list of block types to match (any state of each block matches).
pub struct Mask {
    ids: Vec<u16>,
}

impl Mask {
    pub fn parse(text: &str) -> Result<Self, CommandError> {
        let parts = split_top_level(text);
        if parts.is_empty() {
            return Err(err("Give at least one block to match, e.g. stone or stone,dirt."));
        }
        let mut ids = Vec::new();
        for part in parts {
            if part.contains('%') {
                return Err(err("Percentages are only allowed in the replacement pattern."));
            }
            ids.push(lookup_block(part)?.id);
        }
        Ok(Self { ids })
    }

    pub fn matches(&self, block_id: u16) -> bool {
        self.ids.contains(&block_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{Pattern, split_top_level, split_weight};

    #[test]
    fn splits_plain_lists() {
        assert_eq!(split_top_level("stone,dirt"), vec!["stone", "dirt"]);
        assert_eq!(
            split_top_level("50%stone,50%stone_bricks"),
            vec!["50%stone", "50%stone_bricks"]
        );
    }

    #[test]
    fn keeps_commas_inside_properties() {
        assert_eq!(
            split_top_level("oak_stairs[facing=north,half=top],stone"),
            vec!["oak_stairs[facing=north,half=top]", "stone"]
        );
    }

    #[test]
    fn parses_weights() {
        assert_eq!(split_weight("50%stone"), Ok((50.0, "stone")));
        assert_eq!(split_weight("2.5%dirt"), Ok((2.5, "dirt")));
        assert_eq!(split_weight("stone"), Ok((1.0, "stone")));
        assert!(split_weight("abc%stone").is_err());
        assert!(split_weight("0%stone").is_err());
        assert!(split_weight("50%").is_err());
    }

    #[test]
    fn single_entry_always_wins() {
        let pattern = Pattern::from_entries(vec![(1.0, 7)]);
        assert_eq!(pattern.pick((1, 2, 3), 9), 7);
    }

    #[test]
    fn weighted_split_is_roughly_even() {
        let pattern = Pattern::from_entries(vec![(50.0, 1), (50.0, 2)]);
        let mut first = 0;
        for x in 0..40 {
            for z in 0..40 {
                if pattern.pick((x, 0, z), 3) == 1 {
                    first += 1;
                }
            }
        }
        assert!((700..=900).contains(&first), "got {first} of 1600");
    }

    #[test]
    fn skewed_weights_are_respected() {
        let pattern = Pattern::from_entries(vec![(90.0, 1), (10.0, 2)]);
        let mut second = 0;
        for x in 0..40 {
            for z in 0..40 {
                if pattern.pick((x, 0, z), 5) == 2 {
                    second += 1;
                }
            }
        }
        assert!((80..=240).contains(&second), "got {second} of 1600");
    }
}
