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
use clap::Parser;

use resources::*;
use ship::spawn::STARTING_SIM_TICK;
use ship::PlanningTuning;

/// System set for world setup (islands + ships), so other Startup systems can order after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetupWorld;

/// Default number of archipelago islands.
const DEFAULT_NUM_ISLANDS: usize = 50;
/// Base world size for the default island count.
const BASE_WORLD_SIZE: f32 = 5000.0;
/// Ships per island ratio (initial fleet).
const SHIPS_PER_ISLAND: f32 = 2.0;

#[derive(Parser)]
#[command(name = "Archipelago", about = "An economic trade simulation")]
struct Cli {
    /// Number of archipelago islands
    #[arg(long, default_value_t = DEFAULT_NUM_ISLANDS)]
    islands: usize,

    /// Disable the mainland island
    #[arg(long, default_value_t = false)]
    no_mainland: bool,
}

fn main() {
    let cli = Cli::parse();

    let num_islands = cli.islands.max(2);
    let scale = (num_islands as f32 / DEFAULT_NUM_ISLANDS as f32).sqrt();
    let world_size = BASE_WORLD_SIZE * scale;
    let num_ships = (num_islands as f32 * SHIPS_PER_ISLAND).round() as usize;
    let mainland = !cli.no_mainland;
    let total_islands = if mainland {
        num_islands + 1
    } else {
        num_islands
    };

    let config = WorldConfig {
        num_islands,
        world_size,
        num_ships,
        total_islands,
        mainland_island_id: if mainland { Some(num_islands) } else { None },
    };

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
        .init_resource::<HeatmapOverlay>()
        .insert_resource(config)
        .add_plugins(simulation::SimulationPlugin)
        .add_plugins(rendering::RenderingPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(input::InputPlugin)
        .add_systems(Startup, setup_world.in_set(SetupWorld))
        .run();
}

fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    config: Res<WorldConfig>,
) {
    let mut rng = ::rand::thread_rng();

    // Pre-create shared meshes.
    let clipper_mesh = meshes.add(RegularPolygon::new(8.0, 3));
    let freighter_mesh = meshes.add(Rectangle::new(15.0, 5.0));
    let shorthaul_mesh = meshes.add(Circle::new(5.0));

    // Spawn islands (each gets a unique mesh and color).
    let island_seed_data = island::spawn::spawn_islands(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut rng,
        &config,
    );

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
        &config,
    );
}
