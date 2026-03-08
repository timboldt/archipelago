//! Island entity spawning.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use rand::Rng;

use crate::components::{
    Commodity, IslandBaseColor, IslandId, IslandMarker, MainlandMarker, MarketLedger, Position,
    PriceLedger,
};
use crate::island::IslandEconomy;
use crate::resources::{IslandEntityMap, IslandPositions, RouteHistory, WorldConfig};

/// Buffer distance from world edge when placing islands.
const ISLAND_SPAWN_MARGIN: f32 = 200.0;
/// Minimum distance required between any two islands.
const MIN_ISLAND_SPAWN_DISTANCE: f32 = 140.0;
/// Maximum random placement retries before giving up on an island.
const ISLAND_POSITION_ATTEMPTS: usize = 40;

/// Rolling window (in ticks) for tracking route congestion / recent departures.
pub const ROUTE_HISTORY_WINDOW_TICKS: usize = 10;

/// Base visual radius for an island with average capacity.
const BASE_ISLAND_RADIUS: f32 = 12.0;
/// Number of vertices around the island polygon.
const ISLAND_POLYGON_VERTS: usize = 8;
/// How much each vertex radius can deviate (fraction of base radius).
const ISLAND_JAGGEDNESS: f32 = 0.35;

/// Generate a random irregular polygon mesh for an island.
///
/// `scale` controls overall size (1.0 = average). Vertices are perturbed
/// radially to create a natural coastline shape.
fn make_island_mesh(rng: &mut impl Rng, scale: f32) -> Mesh {
    let radius = BASE_ISLAND_RADIUS * scale;
    let n = ISLAND_POLYGON_VERTS;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n + 1);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n + 1);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n + 1);

    // Center vertex.
    positions.push([0.0, 0.0, 0.0]);
    normals.push([0.0, 0.0, 1.0]);
    uvs.push([0.5, 0.5]);

    for i in 0..n {
        let angle = std::f32::consts::TAU * (i as f32) / (n as f32);
        let r = radius * (1.0 + rng.gen_range(-ISLAND_JAGGEDNESS..ISLAND_JAGGEDNESS));
        positions.push([angle.cos() * r, angle.sin() * r, 0.0]);
        normals.push([0.0, 0.0, 1.0]);
        uvs.push([0.5 + angle.cos() * 0.5, 0.5 + angle.sin() * 0.5]);
    }

    // Triangle fan from center.
    let mut indices: Vec<u32> = Vec::with_capacity(n * 3);
    for i in 0..n {
        indices.push(0);
        indices.push((i + 1) as u32);
        indices.push(((i + 1) % n + 1) as u32);
    }

    Mesh::new(PrimitiveTopology::TriangleList, Default::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// Generate island positions along a Caribbean-style arc with scatter.
///
/// The arc sweeps a randomised portion of an ellipse. Islands are placed
/// along the arc with uniform perpendicular scatter and a minimum-distance
/// check.
pub fn generate_arc_positions(rng: &mut impl Rng, config: &WorldConfig) -> Vec<Vec2> {
    let world_size = config.world_size;
    let num_islands = config.num_islands;

    let center = Vec2::new(world_size * 0.5, world_size * 0.5);
    // Ellipse radii: one axis is fixed, the other varies for eccentricity.
    let radius_a = world_size * 0.38;
    let radius_b = world_size * rng.gen_range(0.2..0.38);
    // Randomize arc length (150–240 degrees) and starting position.
    let arc_length: f32 = rng.gen_range(std::f32::consts::PI * 0.83..std::f32::consts::PI * 1.33);
    let arc_start: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
    let arc_end: f32 = arc_start + arc_length;
    let arc_spread = 1200.0; // perpendicular scatter from the arc spine

    let mut positions: Vec<Vec2> = Vec::with_capacity(num_islands);

    for _ in 0..num_islands {
        let mut best = Vec2::ZERO;
        let mut best_min_dist = -1.0_f32;

        for _ in 0..ISLAND_POSITION_ATTEMPTS {
            // Pick a random angle along the arc.
            let t: f32 = rng.gen_range(arc_start..arc_end);
            // Base point on the arc.
            let spine = center + Vec2::new(t.cos() * radius_a, t.sin() * radius_b);
            // Uniform scatter perpendicular to the arc.
            let offset = rng.gen_range(-arc_spread..arc_spread);
            let tangent = Vec2::new(-t.sin(), t.cos());
            let candidate = spine + tangent * offset;

            // Clamp inside world margins.
            let candidate = Vec2::new(
                candidate
                    .x
                    .clamp(ISLAND_SPAWN_MARGIN, world_size - ISLAND_SPAWN_MARGIN),
                candidate
                    .y
                    .clamp(ISLAND_SPAWN_MARGIN, world_size - ISLAND_SPAWN_MARGIN),
            );

            let min_dist = positions
                .iter()
                .map(|p| candidate.distance(*p))
                .fold(f32::INFINITY, f32::min);

            if min_dist >= MIN_ISLAND_SPAWN_DISTANCE {
                best = candidate;
                best_min_dist = min_dist;
                break;
            }
            if min_dist > best_min_dist {
                best_min_dist = min_dist;
                best = candidate;
            }
        }

        let _ = best_min_dist; // suppress unused warning
        positions.push(best);
    }

    positions
}

/// Spawn island entities and insert shared resources.
///
/// Returns seed data `(position, economy_clone, ledger_clone)` per island,
/// needed by ship spawning to seed initial market views.
/// Pick an island color based on its dominant production.
///
/// Grain-heavy → warm sandy tan, Timber → dark forest green,
/// Iron → rocky grey-brown, Spices → warm terracotta,
/// Tools → muted olive. All with slight random variation.
fn island_color(economy: &IslandEconomy, rng: &mut impl Rng) -> Color {
    let rates = &economy.production_rates;
    let dominant = [
        Commodity::Grain,
        Commodity::Timber,
        Commodity::Iron,
        Commodity::Spices,
    ]
    .into_iter()
    .max_by(|a, b| rates[a.idx()].partial_cmp(&rates[b.idx()]).unwrap())
    .unwrap();

    let v = rng.gen_range(-0.04_f32..0.04); // slight per-island variation
    match dominant {
        Commodity::Grain => Color::srgb(0.76 + v, 0.68 + v, 0.42 + v), // sandy tan
        Commodity::Timber => Color::srgb(0.18 + v, 0.42 + v, 0.15 + v), // dark forest green
        Commodity::Iron => Color::srgb(0.45 + v, 0.42 + v, 0.38 + v),  // rocky grey-brown
        Commodity::Spices => Color::srgb(0.65 + v, 0.38 + v, 0.22 + v), // terracotta
        Commodity::Tools => Color::srgb(0.40 + v, 0.45 + v, 0.30 + v), // muted olive
    }
}

pub fn spawn_islands(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    rng: &mut impl Rng,
    config: &WorldConfig,
) -> Vec<(Vec2, IslandEconomy, PriceLedger)> {
    let total_islands = config.total_islands;
    let island_positions = generate_arc_positions(rng, config);

    let mut entity_map = Vec::with_capacity(total_islands);
    let mut cached_positions = Vec::with_capacity(total_islands);
    let mut island_seed_data: Vec<(Vec2, IslandEconomy, PriceLedger)> = Vec::new();

    for (id, pos) in island_positions.iter().enumerate() {
        let (economy, ledger) = IslandEconomy::new(id, total_islands, rng);

        // Scale the island visual by population_capacity relative to a
        // typical mid-range island (~100 pop capacity).
        let scale = (economy.population_capacity / 100.0).sqrt().clamp(0.5, 2.5);
        let mesh = meshes.add(make_island_mesh(rng, scale));
        let color = island_color(&economy, rng);
        let material = materials.add(color);

        island_seed_data.push((
            *pos,
            IslandEconomy::clone_for_seeding(&economy),
            ledger.clone(),
        ));

        let entity = commands
            .spawn((
                IslandMarker,
                IslandId(id),
                economy,
                MarketLedger(ledger),
                Position(*pos),
                IslandBaseColor(color),
                Mesh2d(mesh),
                MeshMaterial2d(material),
                Transform::from_translation(pos.extend(0.0)),
            ))
            .id();
        entity_map.push(entity);
        cached_positions.push(*pos);
    }

    // Spawn mainland if enabled.
    if let Some(mainland_id) = config.mainland_island_id {
        let (mainland_economy, mainland_ledger, mainland_pos) =
            create_mainland(mainland_id, total_islands, config, &island_positions, rng);

        let scale = (mainland_economy.population_capacity / 5.0)
            .sqrt()
            .clamp(0.5, 5.0);
        let mesh = meshes.add(make_island_mesh(rng, scale));
        // Mainland gets a distinct brownish color.
        let v = rng.gen_range(-0.02_f32..0.02);
        let mainland_color = Color::srgb(0.55 + v, 0.45 + v, 0.30 + v);
        let material = materials.add(mainland_color);

        island_seed_data.push((
            mainland_pos,
            IslandEconomy::clone_for_seeding(&mainland_economy),
            mainland_ledger.clone(),
        ));

        let entity = commands
            .spawn((
                IslandMarker,
                MainlandMarker,
                IslandId(mainland_id),
                mainland_economy,
                MarketLedger(mainland_ledger),
                Position(mainland_pos),
                IslandBaseColor(mainland_color),
                Mesh2d(mesh),
                MeshMaterial2d(material),
                Transform::from_translation(mainland_pos.extend(0.0)),
            ))
            .id();
        entity_map.push(entity);
        cached_positions.push(mainland_pos);
    }

    commands.insert_resource(IslandEntityMap(entity_map));
    commands.insert_resource(IslandPositions(cached_positions));
    commands.insert_resource(RouteHistory {
        recent_route_departures: vec![vec![0.0; total_islands]; total_islands],
        route_departure_history: vec![
            vec![vec![0; total_islands]; total_islands];
            ROUTE_HISTORY_WINDOW_TICKS
        ],
        cursor: 0,
    });

    island_seed_data
}

/// Create a mainland island economy — as large as all archipelago islands combined,
/// no specialization, cannot produce spices.
fn create_mainland(
    id: usize,
    total_islands: usize,
    config: &WorldConfig,
    island_positions: &[Vec2],
    rng: &mut impl Rng,
) -> (IslandEconomy, PriceLedger, Vec2) {
    let n = config.num_islands as f32;

    // Pick a random direction, find the outermost island in that direction,
    // then place mainland 2000-3000 units beyond it.
    let archipelago_center = island_positions.iter().copied().sum::<Vec2>() / n;
    let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
    let direction = Vec2::new(angle.cos(), angle.sin());

    // Find the island farthest along the chosen direction (highest dot product).
    let edge_island = island_positions
        .iter()
        .copied()
        .max_by(|a, b| {
            let da = (*a - archipelago_center).dot(direction);
            let db = (*b - archipelago_center).dot(direction);
            da.partial_cmp(&db).unwrap()
        })
        .unwrap();

    // Place the mainland 2000-3000 units from that edge island.
    let target_dist: f32 = rng.gen_range(2000.0..3000.0);
    let pos = edge_island + direction * target_dist;

    // Create a normal island economy, then scale it up.
    let (mut economy, ledger) = IslandEconomy::new(id, total_islands, rng);

    // Scale economy to be roughly equivalent to all other islands combined.
    // Balanced production (no specialization focus), no spices.
    economy.production_rates = [
        rng.gen_range(1.2..2.0) * n, // Grain
        rng.gen_range(0.8..1.5) * n, // Timber
        rng.gen_range(0.6..1.2) * n, // Iron
        0.0,                         // Tools (fabricated, not produced)
        0.0,                         // Spices — mainland cannot make spices
    ];
    economy.consumption_rates = [
        rng.gen_range(1.0..1.8) * n,   // Grain
        rng.gen_range(0.15..0.3) * n,  // Timber
        rng.gen_range(0.15..0.3) * n,  // Iron
        rng.gen_range(0.2..0.4) * n,   // Tools
        rng.gen_range(0.06..0.12) * n, // Spices — it consumes spices but can't make them
    ];

    // Scale capacities and population to match combined archipelago.
    let size_factor = n;
    economy.population = 80.0 * n;
    economy.population_capacity = 160.0 * n;
    economy.cash = 5000.0 * n;
    economy.infrastructure_level = 1.5;
    economy.infrastructure_capacity = 3.5;
    economy.infra_credit = 1500.0 * n;

    use crate::components::{Commodity as C, INVENTORY_CARRYING_CAPACITY};
    use strum::IntoEnumIterator;
    for c in C::iter() {
        let idx = c.idx();
        economy.resource_capacity[idx] = INVENTORY_CARRYING_CAPACITY * size_factor * 1.0;
        // Mainland starts low on commodities so it's hungry for trade.
        let fill = match c {
            C::Grain | C::Timber | C::Iron => 0.2,
            _ => 0.1,
        };
        economy.inventory[idx] = economy.resource_capacity[idx] * fill;
    }

    // Reset labor allocation for the new production rates.
    economy.labor_allocation =
        IslandEconomy::initial_labor_allocation_for(&economy.production_rates);

    (economy, ledger, pos)
}
