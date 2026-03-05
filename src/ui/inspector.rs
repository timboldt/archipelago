//! Ship and island inspector panels.

use bevy::prelude::*;

use crate::components::{
    IslandId, IslandMarker, Resource, ShipArchetype,
    ShipMarker, ShipMovement, ShipProfile, ShipTrading,
};
use crate::island::IslandEconomy;
use crate::resources::SelectionState;

#[derive(Component)]
pub struct ShipInspectorText;

#[derive(Component)]
pub struct IslandInspectorText;

pub fn update_ship_inspector(
    mut commands: Commands,
    mut inspector_q: Query<(Entity, &mut Text), With<ShipInspectorText>>,
    ships: Query<
        (Entity, &ShipMovement, &ShipTrading, &ShipProfile),
        With<ShipMarker>,
    >,
    selection: Res<SelectionState>,
    ship_count: Query<(), With<ShipMarker>>,
) {
    // Ensure inspector exists.
    if inspector_q.is_empty() {
        commands.spawn((
            ShipInspectorText,
            Text::new(""),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                top: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.82)),
        ));
        return;
    }

    let Ok((_, mut text)) = inspector_q.single_mut() else {
        return;
    };

    let total_ships = ship_count.iter().count();
    let selected_idx = selection.selected_ship_index;

    // Find the ship at the selected index.
    let mut ships_vec: Vec<_> = ships.iter().collect();
    // Sort by entity for stable ordering.
    ships_vec.sort_by_key(|(e, _, _, _)| *e);

    let Some(&(_, movement, trading, profile)) = ships_vec.get(selected_idx) else {
        **text = "No ships".to_string();
        return;
    };

    let archetype_label = match profile.archetype {
        ShipArchetype::Clipper => "Clipper",
        ShipArchetype::Freighter => "Freighter",
        ShipArchetype::Shorthaul => "Shorthaul",
    };

    let status = if let Some(island_id) = trading.docked_at {
        format!("Docked at: {}", island_id)
    } else if let Some(target_id) = movement.target_island_id {
        format!("En route to: {}", target_id)
    } else {
        "Status: Idle".to_string()
    };

    let cargo_text = if let Some((resource, amount)) = trading.cargo {
        let label = IslandEconomy::resource_label(resource);
        format!("Cargo: {} x{:.1}", label, amount)
    } else {
        "Cargo: Empty".to_string()
    };

    let cargo_vol_used = trading
        .cargo
        .map(|(r, a)| a.max(0.0) * r.volume_per_unit())
        .unwrap_or(0.0);

    let (_, _, labor_mult, _) = crate::ship::ShipState::profile_multipliers_static(profile.archetype);
    let (_, _, _, wear_mult) = crate::ship::ShipState::profile_multipliers_static(profile.archetype);
    let labor_rate = crate::ship::BASE_LABOR_RATE_PUB * labor_mult * (1.20 - 0.35 * profile.efficiency_rating).clamp(0.70, 1.15);
    let wear_rate = crate::ship::BASE_WEAR_RATE_PUB * wear_mult * (1.20 - 0.40 * profile.efficiency_rating).clamp(0.65, 1.15);

    let mut s = String::new();
    s.push_str("Selected Ship\n");
    s.push_str(&format!("  Ship: {}/{}\n", selected_idx + 1, total_ships));
    s.push_str(&format!("  Archetype: {}\n", archetype_label));
    s.push_str(&format!("  {}\n", status));
    s.push_str(&format!("  Speed: {:.1}\n", movement.speed));
    s.push_str(&format!("  Cargo vol: {:.1}/{:.1}\n", cargo_vol_used, profile.max_cargo_volume));
    s.push_str(&format!("  Labor/Wear: {:.4}/{:.4}\n", labor_rate, wear_rate));
    s.push_str(&format!("  Cash: {:.1}\n", trading.cash));
    s.push_str(&format!("  {}\n", cargo_text));
    s.push_str("  [ / ]: Prev / Next ship\n");

    **text = s;
}

pub fn update_island_inspector(
    mut commands: Commands,
    mut inspector_q: Query<(Entity, &mut Text), With<IslandInspectorText>>,
    islands: Query<(&IslandId, &IslandEconomy), With<IslandMarker>>,
    selection: Res<SelectionState>,
) {
    if inspector_q.is_empty() {
        commands.spawn((
            IslandInspectorText,
            Text::new(""),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                top: Val::Px(260.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.82)),
        ));
        return;
    }

    let Ok((_, mut text)) = inspector_q.single_mut() else {
        return;
    };

    let total_islands = islands.iter().count();
    if total_islands == 0 {
        **text = "No islands".to_string();
        return;
    }

    let selected_idx = selection.selected_island_index.min(total_islands - 1);

    // Find the island at the selected index.
    let mut islands_vec: Vec<_> = islands.iter().collect();
    islands_vec.sort_by_key(|(id, _)| id.0);

    let Some(&(_, economy)) = islands_vec.get(selected_idx) else {
        **text = "No islands".to_string();
        return;
    };

    let mut s = String::new();
    s.push_str("Selected Island\n");
    s.push_str(&format!("  Island: {}/{}\n", selected_idx + 1, total_islands));
    s.push_str(&format!("  Population: {:.0}\n", economy.population.max(0.0)));
    s.push_str(&format!("  Cash: {:.0}\n", economy.cash.max(0.0)));
    s.push_str(&format!("  Infrastructure: {:.2}\n", economy.infrastructure_level.max(0.0)));
    s.push_str(&format!(
        "  Inv G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}\n",
        economy.inventory[Resource::Grain.idx()].max(0.0),
        economy.inventory[Resource::Timber.idx()].max(0.0),
        economy.inventory[Resource::Iron.idx()].max(0.0),
        economy.inventory[Resource::Tools.idx()].max(0.0),
        economy.inventory[Resource::Spices.idx()].max(0.0),
    ));
    s.push_str(&format!(
        "  Price G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}\n",
        economy.local_prices[Resource::Grain.idx()].max(0.0),
        economy.local_prices[Resource::Timber.idx()].max(0.0),
        economy.local_prices[Resource::Iron.idx()].max(0.0),
        economy.local_prices[Resource::Tools.idx()].max(0.0),
        economy.local_prices[Resource::Spices.idx()].max(0.0),
    ));
    s.push_str("  { / }: Prev / Next island\n");

    **text = s;
}
