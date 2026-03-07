//! Simulation plugin and system set configuration.

pub mod docking;
pub mod economy;
pub mod fleet;
pub mod friction;
pub mod movement;
pub mod route_history;

#[cfg(test)]
mod integration_tests;

use bevy::prelude::*;

/// System sets for simulation phase ordering.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimPhase {
    TickAdvance,
    Economy,
    Movement,
    Friction,
    Docking,
    FleetEvolution,
}

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                SimPhase::TickAdvance,
                SimPhase::Economy,
                SimPhase::Movement,
                SimPhase::Friction,
                SimPhase::Docking,
                SimPhase::FleetEvolution,
            )
                .chain(),
        );

        app.add_systems(
            Update,
            (
                route_history::advance_tick.in_set(SimPhase::TickAdvance),
                route_history::rebuild_island_positions.in_set(SimPhase::TickAdvance),
                economy::update_island_economy.in_set(SimPhase::Economy),
                movement::move_ships.in_set(SimPhase::Movement),
                friction::apply_maritime_friction.in_set(SimPhase::Friction),
                docking::process_docked_ships.in_set(SimPhase::Docking),
                fleet::evolve_fleet.in_set(SimPhase::FleetEvolution),
            ),
        );
    }
}
