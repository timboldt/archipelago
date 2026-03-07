//! All Bevy Resource definitions for the Archipelago simulation.

use bevy::prelude::*;

use crate::ship::PlanningTuning;

/// Current simulation tick counter.
#[derive(Resource)]
pub struct SimulationTick(pub u64);

/// Simulation speed multiplier.
#[derive(Resource)]
pub struct TimeScale(pub f32);

/// Global tuning params (friction, decay, spread).
#[derive(Resource)]
pub struct PlanningTuningRes(pub PlanningTuning);

/// Route departure tracking for congestion awareness.
#[derive(Resource)]
pub struct RouteHistory {
    pub recent_route_departures: Vec<Vec<f32>>,
    pub route_departure_history: Vec<Vec<Vec<u16>>>,
    pub cursor: usize,
}

/// IslandId → Entity lookup for docking.
#[derive(Resource)]
#[allow(dead_code)]
pub struct IslandEntityMap(pub Vec<Entity>);

/// Cached island positions rebuilt each tick.
#[derive(Resource)]
pub struct IslandPositions(pub Vec<Vec2>);

/// Performance HUD data.
#[derive(Resource, Clone, Copy, Default)]
pub struct FrameTimingsRes {
    pub economy_ms: f32,
    pub movement_ms: f32,
    pub dock_ms: f32,
    pub friction_ms: f32,
    pub total_ms: f32,
    pub accum_economy_ms: f32,
    pub accum_movement_ms: f32,
    pub accum_dock_ms: f32,
    pub accum_friction_ms: f32,
    pub accum_total_ms: f32,
    pub samples: u32,
    pub elapsed_secs: f32,
}

pub const PERF_HUD_UPDATE_INTERVAL_SECS: f32 = 1.0;

/// Pre-created mesh handles for spawning ships at runtime.
#[derive(Resource)]
pub struct ShipMeshes {
    pub clipper: Handle<Mesh>,
    pub freighter: Handle<Mesh>,
    pub shorthaul: Handle<Mesh>,
}
