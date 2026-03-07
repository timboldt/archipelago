//! Archipelago — a trade simulation using Bevy ECS.

mod components;
mod input;
mod island;
mod rendering;
mod resources;
mod ship;
mod simulation;
mod ui;

use bevy::prelude::*;

use resources::*;
use ship::spawn::STARTING_SIM_TICK;
use ship::PlanningTuning;

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

    // Pre-create shared meshes.
    let clipper_mesh = meshes.add(RegularPolygon::new(6.0, 3));
    let freighter_mesh = meshes.add(Rectangle::new(10.0, 6.0));
    let shorthaul_mesh = meshes.add(Circle::new(4.0));

    // Spawn islands (each gets a unique mesh and color).
    let island_seed_data =
        island::spawn::spawn_islands(&mut commands, &mut meshes, &mut materials, &mut rng);

    // Store ship mesh handles for runtime spawning (fleet evolution).
    commands.insert_resource(ShipMeshes {
        clipper: clipper_mesh.clone(),
        freighter: freighter_mesh.clone(),
        shorthaul: shorthaul_mesh.clone(),
    });

    // Spawn ships (each gets its own material for per-ship cargo coloring).
    ship::spawn::spawn_ships(
        &mut commands,
        &mut materials,
        &mut rng,
        &island_seed_data,
        clipper_mesh,
        freighter_mesh,
        shorthaul_mesh,
    );
}
