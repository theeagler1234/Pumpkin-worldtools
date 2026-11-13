mod edit;
mod guard;
mod selection;
mod util;

use pumpkin_plugin_api::{
    Context, Plugin, PluginMetadata,
    events::EventPriority,
    permission::{Permission, PermissionDefault, PermissionLevel},
    permissions::FS_WRITE_DATA,
};
use tracing::info;

const GUARD_PERMISSION: &str = "worldtools:guard";

struct WorldToolsPlugin;

impl Plugin for WorldToolsPlugin {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "worldtools".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["worldtools".into()],
            description: "A basic WorldEdit and WorldGuard style plugin.".into(),
            dependencies: Vec::new(),
            // Needed so regions can be saved in the plugin's data folder.
            permissions: vec![FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        for (node, description, level) in [
            (edit_node(), "Use /we selection and edit commands", PermissionLevel::Two),
            (GUARD_PERMISSION, "Manage protected regions with /wg", PermissionLevel::Three),
            (guard::BYPASS_PERMISSION, "Ignore region protection", PermissionLevel::Three),
        ] {
            context.register_permission(&Permission {
                node: node.into(),
                description: description.into(),
                default: PermissionDefault::Op(level),
                children: Vec::new(),
            })?;
        }

        context.register_command(edit::command(), guard::EDIT_PERMISSION);
        context.register_command(guard::command(), GUARD_PERMISSION);

        context.register_event_handler(guard::BreakGuard, EventPriority::High, true)?;
        context.register_event_handler(guard::PlaceGuard, EventPriority::High, true)?;
        context.register_event_handler(guard::InteractGuard, EventPriority::High, true)?;
        context.register_event_handler(guard::PvpGuard, EventPriority::High, true)?;

        guard::load(context.get_data_folder());
        info!("worldtools loaded: /we (WorldEdit) and /wg (WorldGuard)");
        Ok(())
    }

    fn on_unload(&self, _context: Context) -> pumpkin_plugin_api::Result<()> {
        guard::save();
        info!("worldtools unloaded");
        Ok(())
    }
}

fn edit_node() -> &'static str {
    guard::EDIT_PERMISSION
}

pumpkin_plugin_api::register_plugin!(WorldToolsPlugin);
