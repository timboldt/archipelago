//! Docking-phase processing for ships grouped by island.
//!
//! This module keeps the high-contention trade/ledger logic out of `world.rs`
//! while preserving identical simulation ordering and behavior.

use macroquad::prelude::Vec2;
use rayon::prelude::*;

use crate::island::Island;
use crate::ship::{LoadPlanningContext, PlanningTuning, Ship};

use super::{World, MAX_DOCK_SETTLEMENT_STEPS};

/// Result of processing one island's docked ships for a tick.
struct IslandBatchResult {
    ships: Vec<(usize, Ship)>,
    outbound_recent_departures: Vec<f32>,
    departure_targets: Vec<usize>,
}

impl World {
    /// Processes all docked ships for one island using the current tick snapshot.
    fn process_island_docked_batch(
        island_id: usize,
        island: &mut Island,
        mut docked_ships: Vec<(usize, Ship)>,
        mut outbound_recent_departures: Vec<f32>,
        island_positions: &[Vec2],
        planning_tuning: &PlanningTuning,
        tick: u64,
    ) -> IslandBatchResult {
        if docked_ships.is_empty() {
            return IslandBatchResult {
                ships: docked_ships,
                outbound_recent_departures,
                departure_targets: Vec::new(),
            };
        }

        island.mark_seen(tick);
        let mut sold_and_empty = vec![false; docked_ships.len()];
        let mut bankrupt = vec![false; docked_ships.len()];

        for (idx, (_, ship)) in docked_ships.iter_mut().enumerate() {
            let ship_tuning = ship.effective_tuning(planning_tuning);
            ship.begin_dock_tick();
            let load_context = LoadPlanningContext {
                current_island_id: island_id,
                island_positions,
                current_tick: tick,
                tuning: &ship_tuning,
                outbound_recent_departures: &outbound_recent_departures,
            };
            let settled_any = ship.trade_settle_until_stuck(
                island_id,
                island,
                &load_context,
                &ship_tuning,
                MAX_DOCK_SETTLEMENT_STEPS,
            );
            let _ = ship.settle_service_debt(island);
            if settled_any {
                sold_and_empty[idx] = ship.has_no_cargo();
            }
            if ship.is_bankrupt() {
                island.apply_ship_bankruptcy_settlement(ship.removal_cash_settlement());
                bankrupt[idx] = true;
            }
        }

        island.recompute_local_prices(tick);

        for (idx, (_, ship)) in docked_ships.iter_mut().enumerate() {
            if bankrupt[idx] {
                continue;
            }
            let exclude = ship.just_sold_resource();
            let ship_tuning = ship.effective_tuning(planning_tuning);
            let load_context = LoadPlanningContext {
                current_island_id: island_id,
                island_positions,
                current_tick: tick,
                tuning: &ship_tuning,
                outbound_recent_departures: &outbound_recent_departures,
            };
            let _ = ship.trade_load_if_empty(island, exclude, &load_context);
            if ship.cargo_changed_this_dock() {
                let _ = ship.pay_dynamic_docking_tax(island);
            }
            if ship.is_bankrupt() {
                island.apply_ship_bankruptcy_settlement(ship.removal_cash_settlement());
                bankrupt[idx] = true;
            }
        }

        let mut island_ledger_buffer = island.ledger.clone();
        for (idx, (_, ship)) in docked_ships.iter().enumerate() {
            if sold_and_empty[idx] || bankrupt[idx] {
                continue;
            }
            ship.contribute_ledger_to_island_buffer(island_id, &mut island_ledger_buffer);
        }
        island.ledger = island_ledger_buffer;

        let island_ledger_snapshot = island.ledger.clone();
        let mut departure_targets = Vec::new();

        for (idx, (_, ship)) in docked_ships.iter_mut().enumerate() {
            if sold_and_empty[idx] || bankrupt[idx] {
                continue;
            }
            let has_outbound_target = ship
                .target_island()
                .is_some_and(|target_island_id| target_island_id != island_id);
            if !ship.has_no_cargo() && !ship.cargo_changed_this_dock() && has_outbound_target {
                continue;
            }

            let ship_tuning = ship.effective_tuning(planning_tuning);
            ship.sync_ledger_from_snapshot(&island_ledger_snapshot);
            if let Some(target_island_id) = ship.plan_next_island(
                island_id,
                island_positions,
                tick,
                &ship_tuning,
                &outbound_recent_departures,
            ) {
                if target_island_id != island_id {
                    ship.set_target(target_island_id, island_positions[target_island_id]);
                    departure_targets.push(target_island_id);
                    if let Some(slot) = outbound_recent_departures.get_mut(target_island_id) {
                        *slot += 1.0;
                    }
                }
            }
        }

        let ships = docked_ships
            .into_iter()
            .enumerate()
            .filter_map(|(idx, pair)| (!bankrupt[idx]).then_some(pair))
            .collect();

        IslandBatchResult {
            ships,
            outbound_recent_departures,
            departure_targets,
        }
    }

    /// Runs the docking phase: bucket ships by island, process in parallel, reinsert by slot.
    pub(super) fn process_docked_ships(&mut self) {
        let island_count = self.islands.len();
        if island_count == 0 {
            return;
        }

        let mut ship_slots = std::mem::take(&mut self.ships);
        let mut ships_by_island: Vec<Vec<(usize, Ship)>> =
            (0..island_count).map(|_| Vec::new()).collect();

        for (ship_id, slot) in ship_slots.iter_mut().enumerate() {
            let Some(ship) = slot.take() else {
                continue;
            };

            if let Some(island_id) = ship.docked_island() {
                if island_id < island_count {
                    ships_by_island[island_id].push((ship_id, ship));
                    continue;
                }
            }

            *slot = Some(ship);
        }

        let island_positions: Vec<Vec2> = self.islands.iter().map(|island| island.pos).collect();
        let planning_tuning = self.environmental_tuning();
        let tick = self.tick;
        let outbound_seed = self.recent_route_departures.clone();

        let island_results: Vec<IslandBatchResult> = self
            .islands
            .par_iter_mut()
            .zip(ships_by_island.into_par_iter())
            .zip(outbound_seed.into_par_iter())
            .enumerate()
            .map(
                |(island_id, ((island, docked_ships), outbound_recent_departures))| {
                    Self::process_island_docked_batch(
                        island_id,
                        island,
                        docked_ships,
                        outbound_recent_departures,
                        &island_positions,
                        &planning_tuning,
                        tick,
                    )
                },
            )
            .collect();

        for (island_id, result) in island_results.into_iter().enumerate() {
            self.recent_route_departures[island_id] = result.outbound_recent_departures;

            if island_id < self.route_departure_history[self.route_history_cursor].len() {
                for target_island_id in result.departure_targets {
                    if target_island_id
                        < self.route_departure_history[self.route_history_cursor][island_id].len()
                    {
                        let slot = &mut self.route_departure_history[self.route_history_cursor]
                            [island_id][target_island_id];
                        *slot = slot.saturating_add(1);
                    }
                }
            }

            for (ship_id, ship) in result.ships {
                ship_slots[ship_id] = Some(ship);
            }
        }

        self.ships = ship_slots;
        self.ensure_selected_ship_valid();
    }
}
