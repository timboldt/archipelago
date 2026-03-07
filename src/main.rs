//! Archipelago — a trade simulation using Bevy ECS.

mod components;
mod input;
mod island;
mod rendering;
mod resources;
mod ship;
mod simulation;
mod ui;

use ::rand::Rng;
use bevy::prelude::*;

use components::{
    IslandId, IslandMarker, MarketLedger, Position, PriceLedger, ShipArchetype, ShipMarker,
};
use island::spawn::{NUM_ISLANDS, ROUTE_HISTORY_WINDOW_TICKS};
use resources::*;
use ship::spawn::{NUM_SHIPS, STARTING_SIM_TICK};
use ship::{PlanningTuning, ShipState};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Archipelago".to_string(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(
            10.0 / 255.0,
            30.0 / 255.0,
            60.0 / 255.0,
        )))
        .insert_resource(SimulationTick(STARTING_SIM_TICK))
        .insert_resource(TimeScale(1.0))
        .insert_resource(PlanningTuningRes(PlanningTuning {
            global_friction_mult: 1.0,
            info_decay_rate: 0.003,
            market_spread: 0.10,
        }))
        .insert_resource(FrameTimingsRes::default())
        .add_plugins(simulation::SimulationPlugin)
        .add_plugins(rendering::RenderingPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(input::InputPlugin)
        .add_systems(Startup, setup_world)
        .run();
}

fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = ::rand::thread_rng();

    // Pre-create shared meshes for islands and ships.
    let island_mesh = meshes.add(Circle::new(12.0));
    let island_material = materials.add(Color::srgb(0.2, 0.8, 0.3));

    // Ship meshes by archetype.
    let clipper_mesh = meshes.add(RegularPolygon::new(6.0, 3));
    let freighter_mesh = meshes.add(Rectangle::new(10.0, 6.0));
    let shorthaul_mesh = meshes.add(Circle::new(4.0));
    let ship_material = materials.add(Color::srgb(0.9, 0.9, 0.9));

    // Generate island positions along a Caribbean-style arc.
    let island_positions = island::spawn::generate_arc_positions(&mut rng);

    // Create island entities and collect data for ship seeding.
    let mut entity_map = Vec::with_capacity(NUM_ISLANDS);
    let mut cached_positions = Vec::with_capacity(NUM_ISLANDS);
    let mut island_seed_data: Vec<(Vec2, island::IslandEconomy, PriceLedger)> = Vec::new();

    for (id, pos) in island_positions.iter().enumerate() {
        let (economy, ledger) = island::IslandEconomy::new(id, NUM_ISLANDS, &mut rng);

        // Save a copy for ship seeding before moving into entity.
        island_seed_data.push((
            *pos,
            island::IslandEconomy::clone_for_seeding(&economy),
            ledger.clone(),
        ));

        let entity = commands
            .spawn((
                IslandMarker,
                IslandId(id),
                economy,
                MarketLedger(ledger),
                Position(*pos),
                Mesh2d(island_mesh.clone()),
                MeshMaterial2d(island_material.clone()),
                Transform::from_translation(pos.extend(0.0)),
            ))
            .id();
        entity_map.push(entity);
        cached_positions.push(*pos);
    }

    commands.insert_resource(IslandEntityMap(entity_map));
    commands.insert_resource(IslandPositions(cached_positions));
    commands.insert_resource(RouteHistory {
        recent_route_departures: vec![vec![0.0; NUM_ISLANDS]; NUM_ISLANDS],
        route_departure_history: vec![
            vec![vec![0; NUM_ISLANDS]; NUM_ISLANDS];
            ROUTE_HISTORY_WINDOW_TICKS
        ],
        cursor: 0,
    });

    // Store ship mesh handles for runtime spawning (fleet evolution).
    commands.insert_resource(ShipMeshes {
        clipper: clipper_mesh.clone(),
        freighter: freighter_mesh.clone(),
        shorthaul: shorthaul_mesh.clone(),
        material: ship_material.clone(),
    });

    // Spawn ships with seeded market views.
    for i in 0..NUM_SHIPS {
        let speed = rng.gen_range(200.0_f32..500.0);
        let start_island_id = i % NUM_ISLANDS;
        let start_pos = island_seed_data[start_island_id].0;
        let mut ship = ShipState::new(start_pos, speed, NUM_ISLANDS, start_island_id);
        ship.seed_initial_market_view(
            &island_seed_data,
            STARTING_SIM_TICK,
            start_island_id,
            &mut rng,
        );

        let (movement, trading, profile, ship_ledger) = ship.into_components();
        let mesh = match profile.archetype {
            ShipArchetype::Clipper => clipper_mesh.clone(),
            ShipArchetype::Freighter => freighter_mesh.clone(),
            ShipArchetype::Shorthaul => shorthaul_mesh.clone(),
        };
        commands.spawn((
            ShipMarker,
            Position(start_pos),
            movement,
            trading,
            profile,
            ship_ledger,
            Mesh2d(mesh),
            MeshMaterial2d(ship_material.clone()),
            Transform::from_translation(start_pos.extend(1.0)),
        ));
    }
}
