//! Selection highlight system — draws circles around the selected ship and island.

use crate::components::{Position, SelectedIsland, SelectedShip};
use bevy::prelude::*;

#[derive(Component)]
pub struct ShipSelectionHighlight;

#[derive(Component)]
pub struct IslandSelectionHighlight;

pub fn setup_selection_highlights(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<bevy::sprite::ColorMaterial>>,
) {
    let ship_mesh = meshes.add(Annulus::new(12.0, 15.0));
    let island_mesh = meshes.add(Annulus::new(19.0, 24.0));
    let ship_material = materials.add(Color::srgb(1.0, 0.0, 0.0));
    let island_material = materials.add(Color::srgb(1.0, 0.0, 0.0));

    commands.spawn((
        ShipSelectionHighlight,
        Mesh2d(ship_mesh),
        MeshMaterial2d(ship_material),
        Transform::from_xyz(-9999.0, -9999.0, 0.5),
    ));

    commands.spawn((
        IslandSelectionHighlight,
        Mesh2d(island_mesh),
        MeshMaterial2d(island_material),
        Transform::from_xyz(-9999.0, -9999.0, -0.5),
    ));
}

pub fn update_selection_highlights(
    selected_ship: Query<&Position, With<SelectedShip>>,
    selected_island: Query<&Position, (With<SelectedIsland>, Without<SelectedShip>)>,
    mut ship_highlight: Query<
        &mut Transform,
        (
            With<ShipSelectionHighlight>,
            Without<IslandSelectionHighlight>,
        ),
    >,
    mut island_highlight: Query<
        &mut Transform,
        (
            With<IslandSelectionHighlight>,
            Without<ShipSelectionHighlight>,
        ),
    >,
) {
    if let Ok(mut hl) = ship_highlight.single_mut() {
        if let Ok(pos) = selected_ship.single() {
            hl.translation.x = pos.0.x;
            hl.translation.y = pos.0.y;
            hl.translation.z = 0.5;
        } else {
            hl.translation.x = -9999.0;
        }
    }

    if let Ok(mut hl) = island_highlight.single_mut() {
        if let Ok(pos) = selected_island.single() {
            hl.translation.x = pos.0.x;
            hl.translation.y = pos.0.y;
            hl.translation.z = -0.5;
        } else {
            hl.translation.x = -9999.0;
        }
    }
}
