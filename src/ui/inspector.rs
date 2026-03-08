//! Ship and island inspector panels.

use bevy::prelude::*;

use crate::components::{
    Commodity, SelectedIsland, SelectedShip, ShipArchetype, ShipMovement, ShipProfile, ShipTrading,
};
use crate::island::IslandEconomy;

/// Resource tracking whether inspector panels are visible.
#[derive(Resource)]
pub struct InspectorVisible(pub bool);

impl Default for InspectorVisible {
    fn default() -> Self {
        Self(true)
    }
}

pub fn toggle_inspector(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<InspectorVisible>) {
    if keys.just_pressed(KeyCode::KeyI) {
        visible.0 = !visible.0;
    }
}

#[derive(Component)]
pub struct ShipInspectorText;

#[derive(Component)]
pub struct IslandInspectorText;

pub fn update_ship_inspector(
    mut commands: Commands,
    visible: Res<InspectorVisible>,
    mut inspector_q: Query<(Entity, &mut Text, &mut Node), With<ShipInspectorText>>,
    selected_ship: Query<(&ShipMovement, &ShipTrading, &ShipProfile), With<SelectedShip>>,
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
                left: Val::Px(14.0),
                bottom: Val::Px(14.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.82)),
        ));
        return;
    }

    let Ok((_, mut text, mut node)) = inspector_q.single_mut() else {
        return;
    };

    if !visible.0 {
        node.display = Display::None;
        return;
    }

    let Ok((movement, trading, profile)) = selected_ship.single() else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;

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

    let (_, _, labor_mult, _) =
        crate::ship::ShipState::profile_multipliers_static(profile.archetype);
    let (_, _, _, wear_mult) =
        crate::ship::ShipState::profile_multipliers_static(profile.archetype);
    let labor_rate = crate::ship::BASE_LABOR_RATE
        * labor_mult
        * (1.20 - 0.35 * profile.efficiency_rating).clamp(0.70, 1.15);
    let wear_rate = crate::ship::BASE_WEAR_RATE
        * wear_mult
        * (1.20 - 0.40 * profile.efficiency_rating).clamp(0.65, 1.15);

    let mut s = String::new();
    s.push_str("Selected Ship\n");
    s.push_str(&format!("  Archetype: {}\n", archetype_label));
    s.push_str(&format!("  {}\n", status));
    s.push_str(&format!("  Speed: {:.1}\n", movement.speed));
    s.push_str(&format!(
        "  Cargo vol: {:.1}/{:.1}\n",
        cargo_vol_used, profile.max_cargo_volume
    ));
    s.push_str(&format!(
        "  Labor/Wear: {:.4}/{:.4}\n",
        labor_rate, wear_rate
    ));
    let cargo_value = if let Some((_, amount)) = trading.cargo {
        (trading.purchase_price.max(0.0) * amount.max(0.0)).max(0.0)
    } else {
        0.0
    };
    let debt = trading.labor_debt.max(0.0) + trading.wear_debt.max(0.0);
    let wealth = trading.cash + cargo_value - debt;

    s.push_str(&format!("  Cash: {:.1}\n", trading.cash));
    s.push_str(&format!("  Wealth: {:.1}\n", wealth));
    s.push_str(&format!("  {}\n", cargo_text));
    s.push_str("  [ / ]: Prev / Next ship\n");
    s.push_str("  [I] Toggle inspector\n");

    text.0 = s;
}

pub fn update_island_inspector(
    mut commands: Commands,
    visible: Res<InspectorVisible>,
    mut inspector_q: Query<(Entity, &mut Text, &mut Node), With<IslandInspectorText>>,
    selected_island: Query<&IslandEconomy, With<SelectedIsland>>,
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
                left: Val::Px(14.0),
                bottom: Val::Px(14.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.06, 0.12, 0.82)),
        ));
        return;
    }

    let Ok((_, mut text, mut node)) = inspector_q.single_mut() else {
        return;
    };

    if !visible.0 {
        node.display = Display::None;
        return;
    }

    let Ok(economy) = selected_island.single() else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;

    let mut s = String::new();
    s.push_str("Selected Island\n");
    s.push_str(&format!(
        "  Population: {:.0}K\n",
        economy.population.max(0.0)
    ));
    s.push_str(&format!("  Cash: {:.0}\n", economy.cash.max(0.0)));
    s.push_str(&format!(
        "  Infrastructure: {:.2}\n",
        economy.infrastructure_level.max(0.0)
    ));
    s.push_str(&format!(
        "  Inv G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}\n",
        economy.inventory[Commodity::Grain.idx()].max(0.0),
        economy.inventory[Commodity::Timber.idx()].max(0.0),
        economy.inventory[Commodity::Iron.idx()].max(0.0),
        economy.inventory[Commodity::Tools.idx()].max(0.0),
        economy.inventory[Commodity::Spices.idx()].max(0.0),
    ));
    s.push_str(&format!(
        "  Price G/T/I/To/S: {:.0}/{:.0}/{:.0}/{:.0}/{:.0}\n",
        economy.local_prices[Commodity::Grain.idx()].max(0.0),
        economy.local_prices[Commodity::Timber.idx()].max(0.0),
        economy.local_prices[Commodity::Iron.idx()].max(0.0),
        economy.local_prices[Commodity::Tools.idx()].max(0.0),
        economy.local_prices[Commodity::Spices.idx()].max(0.0),
    ));
    s.push_str(&format!(
        "  Labor G/T/I/S: {:.0}%/{:.0}%/{:.0}%/{:.0}%\n",
        economy.labor_allocation[Commodity::Grain.idx()] * 100.0,
        economy.labor_allocation[Commodity::Timber.idx()] * 100.0,
        economy.labor_allocation[Commodity::Iron.idx()] * 100.0,
        economy.labor_allocation[Commodity::Spices.idx()] * 100.0,
    ));
    s.push_str(&format!(
        "  Spice Morale: {:.2}x\n",
        economy.spice_morale_bonus,
    ));
    s.push_str("  { / }: Prev / Next island\n");
    s.push_str("  [I] Toggle inspector\n");

    text.0 = s;
}
