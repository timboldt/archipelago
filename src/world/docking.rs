//! Docking-phase processing for ships grouped by island.
//!
//! This module keeps the high-contention trade/ledger logic out of `world.rs`
//! while preserving identical simulation ordering and behavior.
//!
//! Islands are processed sequentially so that ships never need to leave
//! `World::ships`. Split borrows (`&mut self.islands` / `&mut self.ships`) give
//! the compiler the proof that the two slices are disjoint.

use macroquad::prelude::Vec2;

use crate::ship::LoadPlanningContext;

use super::{World, MAX_DOCK_SETTLEMENT_STEPS};

impl World {
    /// Runs the docking phase sequentially: for each island process its docked
    /// ships in-place, then remove any that went bankrupt.
    pub(super) fn process_docked_ships(&mut self) {
        let island_count = self.islands.len();
        if island_count == 0 {
            return;
        }

        let island_positions: Vec<Vec2> = self.islands.iter().map(|i| i.pos).collect();
        let planning_tuning = self.environmental_tuning();
        let tick = self.tick;

        // Index the docked ships by island up front so we never hold a borrow
        // on `self.ships` while we also borrow `self.islands`.
        let mut docked_by_island: Vec<Vec<usize>> = (0..island_count).map(|_| Vec::new()).collect();
        for (ship_idx, ship) in self.ships.iter().enumerate() {
            if let Some(island_id) = ship.docked_island() {
                if island_id < island_count {
                    docked_by_island[island_id].push(ship_idx);
                }
            }
        }

        // Accumulate departure targets for the route-history ledger.
        let mut all_departure_targets: Vec<(usize, usize)> = Vec::new(); // (from_island, to_island)

        // Bankrupt ships are collected here and removed after all islands are
        // processed so we don't invalidate the docked_by_island indices mid-loop.
        let mut bankrupt_indices: Vec<usize> = Vec::new();

        for (island_id, ship_indices) in docked_by_island.iter().enumerate() {
            if ship_indices.is_empty() {
                continue;
            }

            // Split borrows: islands and ships are separate fields.
            let island = &mut self.islands[island_id];
            island.mark_seen(tick);

            let mut sold_and_empty = vec![false; ship_indices.len()];
            let mut bankrupt_local = vec![false; ship_indices.len()];

            // Pass 1: sell / settle service debt.
            for (local_idx, &ship_idx) in ship_indices.iter().enumerate() {
                let ship = &mut self.ships[ship_idx];
                let ship_tuning = ship.effective_tuning(&planning_tuning);
                ship.begin_dock_tick();
                let settled_any = ship.trade_settle_until_stuck(
                    island_id,
                    island,
                    &ship_tuning,
                    MAX_DOCK_SETTLEMENT_STEPS,
                );
                let _ = ship.settle_service_debt(island);
                if settled_any {
                    sold_and_empty[local_idx] = ship.has_no_cargo();
                }
                if ship.is_bankrupt() {
                    island.apply_ship_bankruptcy_settlement(ship.removal_cash_settlement());
                    bankrupt_local[local_idx] = true;
                    bankrupt_indices.push(ship_idx);
                }
            }

            island.recompute_local_prices(tick);

            // Pass 2: load planning.
            let outbound_recent_departures = self.recent_route_departures[island_id].clone();
            for (local_idx, &ship_idx) in ship_indices.iter().enumerate() {
                if bankrupt_local[local_idx] {
                    continue;
                }
                let ship = &mut self.ships[ship_idx];
                let exclude = ship.just_sold_resource();
                let ship_tuning = ship.effective_tuning(&planning_tuning);
                let load_context = LoadPlanningContext {
                    current_island_id: island_id,
                    island_positions: &island_positions,
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
                    bankrupt_local[local_idx] = true;
                    bankrupt_indices.push(ship_idx);
                }
            }

            // Pass 3: ledger merge — accumulate ship knowledge into island buffer.
            let mut island_ledger_buffer = island.ledger.clone();
            for (local_idx, &ship_idx) in ship_indices.iter().enumerate() {
                if sold_and_empty[local_idx] || bankrupt_local[local_idx] {
                    continue;
                }
                self.ships[ship_idx]
                    .contribute_ledger_to_island_buffer(island_id, &mut island_ledger_buffer);
            }
            island.ledger = island_ledger_buffer;

            // Pass 4: departure planning — sync each ship from the merged ledger.
            let island_ledger_snapshot = island.ledger.clone();
            let mut outbound_for_island = self.recent_route_departures[island_id].clone();

            for (local_idx, &ship_idx) in ship_indices.iter().enumerate() {
                if sold_and_empty[local_idx] || bankrupt_local[local_idx] {
                    continue;
                }
                let ship = &mut self.ships[ship_idx];
                let has_outbound_target = ship.target_island().is_some_and(|t| t != island_id);
                if !ship.has_no_cargo() && !ship.cargo_changed_this_dock() && has_outbound_target {
                    continue;
                }

                let ship_tuning = ship.effective_tuning(&planning_tuning);
                ship.sync_ledger_from_snapshot(&island_ledger_snapshot);
                if let Some(target_island_id) = ship.plan_next_island(
                    island_id,
                    &island_positions,
                    tick,
                    &ship_tuning,
                    &outbound_for_island,
                ) {
                    if target_island_id != island_id {
                        ship.set_target(target_island_id, island_positions[target_island_id]);
                        all_departure_targets.push((island_id, target_island_id));
                        if let Some(slot) = outbound_for_island.get_mut(target_island_id) {
                            *slot += 1.0;
                        }
                    }
                }
            }

            self.recent_route_departures[island_id] = outbound_for_island;
        }

        // Record departure targets in route history.
        let cursor = self.route_history_cursor;
        for (from_island, to_island) in all_departure_targets {
            if from_island < self.route_departure_history[cursor].len()
                && to_island < self.route_departure_history[cursor][from_island].len()
            {
                let slot = &mut self.route_departure_history[cursor][from_island][to_island];
                *slot = slot.saturating_add(1);
            }
        }

        // Remove bankrupt ships. Sort descending so swap_remove preserves
        // the validity of later indices.
        bankrupt_indices.sort_unstable_by(|a, b| b.cmp(a));
        bankrupt_indices.dedup();
        for idx in bankrupt_indices {
            self.ships.swap_remove(idx);
        }

        self.ensure_selected_ship_valid();
    }
}
