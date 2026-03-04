use macroquad::prelude::Vec2;

use crate::island::{Resource, BASE_COSTS, INVENTORY_CARRYING_CAPACITY, RESOURCE_COUNT};

use super::{bid_multiplier, PlanningTuning, Ship};

const UNKNOWN_CASH_CONFIDENCE_SCALE: f32 = 0.70;
const DEFAULT_MARKET_DEPTH_FALLBACK: f32 = 600.0;
const RECENT_BROKE_TICKS: f32 = 180.0;
const BROKE_ISLAND_UTILITY_PENALTY: f32 = 5.5;
const BROKE_CASH_COVERAGE_RATIO: f32 = 0.35;
const BROKE_DESTINATION_BLOCK_CASH: f32 = 1.0;
const BROKE_DESTINATION_BLOCK_MAX_AGE: f32 = 180.0;
const INDUSTRIAL_INFRA_THRESHOLD: f32 = 1.5;
const INDUSTRIAL_INPUT_BONUS_PER_INFRA: f32 = 4.0;
const INDUSTRIAL_INPUT_BONUS_CAP: f32 = 14.0;
const HIGH_PRICE_RISK_WEIGHT: f32 = 0.65;

pub(super) struct UtilityContext<'a> {
    pub island_positions: &'a [Vec2],
    pub max_route_distance: f32,
    pub current_tick: u64,
    pub tuning: &'a PlanningTuning,
    pub outbound_recent_departures: &'a [f32],
}

impl Ship {
    pub(super) fn destination_confidence(
        &self,
        target_id: usize,
        distance: f32,
        current_tick: u64,
        tuning: &PlanningTuning,
        outbound_recent_departures: &[f32],
    ) -> f32 {
        let transit_time = distance / self.speed.max(1.0);
        let data_age = current_tick.saturating_sub(self.ledger[target_id].tick_updated) as f32;
        let base_confidence = (-tuning.info_decay_rate * (data_age + transit_time))
            .exp()
            .clamp(0.05, 1.0);

        let recent_route_flow = outbound_recent_departures
            .get(target_id)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let route_confidence_factor = if recent_route_flow >= 1.0 {
            1.0 / recent_route_flow
        } else {
            1.0
        };

        (base_confidence * route_confidence_factor).clamp(0.02, 1.0)
    }

    pub(super) fn calculate_utility(
        &self,
        resource: Resource,
        target_id: usize,
        buy_price: f32,
        lot_size: f32,
        context: &UtilityContext<'_>,
    ) -> f32 {
        if target_id >= self.ledger.len() || target_id >= context.island_positions.len() {
            return f32::NEG_INFINITY;
        }

        if !buy_price.is_finite() || buy_price <= 0.0 {
            return f32::NEG_INFINITY;
        }

        let quoted_sell_price = self.ledger[target_id].prices[resource.idx()];
        let quoted_inventory = self.ledger[target_id].inventories[resource.idx()].max(0.0);
        let has_quoted_sell_price = quoted_sell_price.is_finite() && quoted_sell_price > 0.0;
        let median_market_price = self.median_price_for_resource(resource);
        let quoted_bid_price = quoted_sell_price * bid_multiplier(context.tuning.market_spread);
        let expected_sell_price = if has_quoted_sell_price {
            quoted_bid_price
        } else if median_market_price > 0.0 {
            median_market_price * bid_multiplier(context.tuning.market_spread)
        } else {
            buy_price
        };

        let distance = (self.pos - context.island_positions[target_id]).length();
        if distance > context.max_route_distance {
            return f32::NEG_INFINITY;
        }
        let transit_time = distance / self.speed.max(1.0);
        let mut confidence = self.destination_confidence(
            target_id,
            distance,
            context.current_tick,
            context.tuning,
            context.outbound_recent_departures,
        );
        if !has_quoted_sell_price {
            confidence = (confidence * 0.45).clamp(0.02, 1.0);
        }

        let quoted_island_cash = self.ledger[target_id].cash;
        let has_quoted_cash = quoted_island_cash.is_finite() && quoted_island_cash > 0.0;
        let data_age = context
            .current_tick
            .saturating_sub(self.ledger[target_id].tick_updated) as f32;
        let recently_broke_destination = quoted_island_cash <= BROKE_DESTINATION_BLOCK_CASH
            && data_age <= BROKE_DESTINATION_BLOCK_MAX_AGE;
        if recently_broke_destination {
            // Keep route eligible for barter-driven flow, but discount confidence.
            confidence = (confidence * 0.55).clamp(0.02, 1.0);
        }
        let fallback_cash = self
            .median_island_cash()
            .unwrap_or(DEFAULT_MARKET_DEPTH_FALLBACK)
            .max(DEFAULT_MARKET_DEPTH_FALLBACK);
        let market_depth_cash = if has_quoted_cash {
            quoted_island_cash
        } else {
            confidence = (confidence * UNKNOWN_CASH_CONFIDENCE_SCALE).clamp(0.02, 1.0);
            fallback_cash
        };

        let available_storage = (INVENTORY_CARRYING_CAPACITY - quoted_inventory).max(0.0);
        let effective_lot_size = lot_size.max(0.0).min(available_storage);
        if effective_lot_size <= 0.0 {
            return f32::NEG_INFINITY;
        }

        let gross_expected_revenue = expected_sell_price * effective_lot_size;
        let real_expected_revenue = if has_quoted_cash {
            gross_expected_revenue.min(market_depth_cash * 0.9)
        } else {
            // Cash-starved destinations can still settle value via barter swaps.
            gross_expected_revenue
        };
        let real_expected_profit = real_expected_revenue - (buy_price * effective_lot_size);

        let average_base_cost = BASE_COSTS.iter().copied().sum::<f32>() / RESOURCE_COUNT as f32;
        let relative_price = (buy_price / average_base_cost).max(0.0);
        let price_risk_penalty = (relative_price - 1.0).max(0.0) * HIGH_PRICE_RISK_WEIGHT;
        let price_risk_factor = (1.0 / (1.0 + price_risk_penalty)).clamp(0.35, 1.0);
        confidence *= price_risk_factor;

        let expected_profit = real_expected_profit * confidence;
        let estimated_load_ratio = (effective_lot_size * resource.volume_per_unit().max(0.01)
            / self.max_cargo_volume.max(0.01))
        .clamp(0.0, 1.0);
        let wear_multiplier = 1.0 + estimated_load_ratio * super::HEAVY_LOAD_WEAR_MULTIPLIER;
        let rigging_repair_cost =
            distance * self.wear_rate() * context.tuning.global_friction_mult * wear_multiplier;
        let labor_provisions_trip_cost =
            transit_time * self.labor_rate() * context.tuning.global_friction_mult;
        let capital_carry_cost = buy_price
            * effective_lot_size
            * transit_time
            * self.labor_rate()
            * context.tuning.global_friction_mult;
        let trip_cost_basis = rigging_repair_cost + labor_provisions_trip_cost + capital_carry_cost;
        let staleness_risk_cost = (1.0 - confidence) * trip_cost_basis / self.risk_tolerance();

        let broke_revenue_threshold = gross_expected_revenue * BROKE_CASH_COVERAGE_RATIO;
        let recent_broke_factor = (1.0 - data_age / RECENT_BROKE_TICKS).clamp(0.0, 1.0);
        let broke_penalty = if has_quoted_cash && quoted_island_cash < broke_revenue_threshold {
            BROKE_ISLAND_UTILITY_PENALTY * recent_broke_factor
        } else {
            0.0
        };

        let industrial_bonus = if resource == Resource::Iron || resource == Resource::Timber {
            let infra_excess =
                (self.ledger[target_id].infrastructure_level - INDUSTRIAL_INFRA_THRESHOLD).max(0.0);
            (infra_excess * INDUSTRIAL_INPUT_BONUS_PER_INFRA).min(INDUSTRIAL_INPUT_BONUS_CAP)
        } else {
            0.0
        };

        expected_profit
            - rigging_repair_cost
            - labor_provisions_trip_cost
            - capital_carry_cost
            - staleness_risk_cost
            + industrial_bonus
            - broke_penalty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::Vec2;
    use rstest::rstest;

    fn make_ship_with_ledger(num_islands: usize) -> Ship {
        Ship::new(Vec2::new(0.0, 0.0), 300.0, num_islands, 0)
    }

    fn default_tuning() -> PlanningTuning {
        PlanningTuning::default()
    }

    // ── bid/ask multipliers ───────────────────────────────────────────────

    #[rstest]
    #[case(0.0, 1.0, 1.0)] // zero spread: bid == ask == 1.0
    #[case(0.1, 0.95, 1.05)] // default spread
    #[case(0.5, 0.75, 1.25)]
    #[case(1.0, 0.5, 1.5)]
    fn bid_ask_multiplier_parametric(
        #[case] spread: f32,
        #[case] expected_bid: f32,
        #[case] expected_ask: f32,
    ) {
        let bid = super::super::bid_multiplier(spread);
        let ask = super::super::ask_multiplier(spread);
        assert!(
            (bid - expected_bid).abs() < 1e-5,
            "bid mismatch: {bid} vs {expected_bid}"
        );
        assert!(
            (ask - expected_ask).abs() < 1e-5,
            "ask mismatch: {ask} vs {expected_ask}"
        );
    }

    // ── destination_confidence ────────────────────────────────────────────

    #[rstest]
    #[case(0, 1.0)] // fresh data: high confidence
    #[case(100, 0.74)] // moderate age
    #[case(500, 0.22)] // old data
    #[case(2000, 0.05)] // very stale: clamps to floor
    fn destination_confidence_decays_with_data_age(
        #[case] data_age_ticks: u64,
        #[case] expected_approx: f32,
    ) {
        let mut ship = make_ship_with_ledger(2);
        // Set ledger so data_age = data_age_ticks ticks
        ship.ledger[1].tick_updated = 0;
        let current_tick = data_age_ticks;
        let tuning = default_tuning(); // info_decay_rate = 0.003
        let departures = [0.0_f32, 0.0_f32];

        let confidence = ship.destination_confidence(1, 0.0, current_tick, &tuning, &departures);

        assert!(
            (confidence - expected_approx).abs() < 0.05,
            "tick={data_age_ticks}: confidence={confidence:.3} expected≈{expected_approx}"
        );
    }

    #[rstest]
    #[case(0.0, 1.0)] // no competing ships: no reduction
    #[case(1.0, 1.0)] // exactly 1 departure: no reduction (threshold is >= 1.0 → 1/1 = 1)
    #[case(2.0, 0.5)] // 2 departures: half
    #[case(4.0, 0.25)] // 4 departures: quarter
    fn destination_confidence_crowded_route_reduces_confidence(
        #[case] recent_flow: f32,
        #[case] expected_route_factor: f32,
    ) {
        let mut ship = make_ship_with_ledger(2);
        ship.ledger[1].tick_updated = 100;
        let departures = [0.0_f32, recent_flow];
        let tuning = default_tuning();

        // Fresh data, zero distance → base_confidence ≈ 1.0
        let confidence = ship.destination_confidence(1, 0.0, 100, &tuning, &departures);

        let expected = (1.0_f32 * expected_route_factor).clamp(0.02, 1.0);
        assert!(
            (confidence - expected).abs() < 0.05,
            "flow={recent_flow}: confidence={confidence:.3} expected≈{expected:.3}"
        );
    }

    // ── calculate_utility ────────────────────────────────────────────────

    fn setup_utility_context<'a>(
        positions: &'a [Vec2],
        tuning: &'a PlanningTuning,
        departures: &'a [f32],
        max_dist: f32,
    ) -> UtilityContext<'a> {
        UtilityContext {
            island_positions: positions,
            max_route_distance: max_dist,
            current_tick: 100,
            tuning,
            outbound_recent_departures: departures,
        }
    }

    #[test]
    fn calculate_utility_profitable_route_is_positive() {
        let mut ship = make_ship_with_ledger(2);
        ship.cash = 10_000.0;
        // Destination: high sell price, stocked with cash, close
        ship.ledger[1].prices[Resource::Grain.idx()] = 200.0;
        ship.ledger[1].inventories[Resource::Grain.idx()] = 0.0;
        ship.ledger[1].cash = 100_000.0;
        ship.ledger[1].tick_updated = 100;

        let positions = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let tuning = default_tuning();
        let departures = [0.0_f32, 0.0_f32];
        let ctx = setup_utility_context(&positions, &tuning, &departures, 10_000.0);

        let utility = ship.calculate_utility(Resource::Grain, 1, 10.0, 5.0, &ctx);

        assert!(utility > 0.0, "expected positive utility, got {utility}");
    }

    #[test]
    fn calculate_utility_loss_route_is_negative() {
        let mut ship = make_ship_with_ledger(2);
        ship.cash = 10_000.0;
        // Buy expensive, sell cheaper, long distance
        ship.ledger[1].prices[Resource::Grain.idx()] = 5.0; // sell price well below buy
        ship.ledger[1].inventories[Resource::Grain.idx()] = 0.0;
        ship.ledger[1].cash = 100_000.0;
        ship.ledger[1].tick_updated = 100;

        let positions = [Vec2::new(0.0, 0.0), Vec2::new(2000.0, 0.0)];
        let tuning = default_tuning();
        let departures = [0.0_f32, 0.0_f32];
        let ctx = setup_utility_context(&positions, &tuning, &departures, 10_000.0);

        let utility = ship.calculate_utility(Resource::Grain, 1, 500.0, 5.0, &ctx);

        assert!(utility < 0.0, "expected negative utility, got {utility}");
    }

    #[test]
    fn calculate_utility_exceeds_max_distance_returns_neg_inf() {
        let mut ship = make_ship_with_ledger(2);
        ship.ledger[1].prices[Resource::Grain.idx()] = 200.0;
        ship.ledger[1].cash = 100_000.0;
        ship.ledger[1].tick_updated = 100;

        let positions = [Vec2::new(0.0, 0.0), Vec2::new(5000.0, 0.0)];
        let tuning = default_tuning();
        let departures = [0.0_f32, 0.0_f32];
        // max_route_distance smaller than actual distance
        let ctx = setup_utility_context(&positions, &tuning, &departures, 100.0);

        let utility = ship.calculate_utility(Resource::Grain, 1, 10.0, 5.0, &ctx);

        assert_eq!(utility, f32::NEG_INFINITY);
    }

    #[test]
    fn calculate_utility_full_destination_inventory_returns_neg_inf() {
        let mut ship = make_ship_with_ledger(2);
        ship.ledger[1].prices[Resource::Grain.idx()] = 200.0;
        // Destination inventory already at capacity
        ship.ledger[1].inventories[Resource::Grain.idx()] = INVENTORY_CARRYING_CAPACITY;
        ship.ledger[1].cash = 100_000.0;
        ship.ledger[1].tick_updated = 100;

        let positions = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let tuning = default_tuning();
        let departures = [0.0_f32, 0.0_f32];
        let ctx = setup_utility_context(&positions, &tuning, &departures, 10_000.0);

        let utility = ship.calculate_utility(Resource::Grain, 1, 10.0, 5.0, &ctx);

        assert_eq!(utility, f32::NEG_INFINITY);
    }
}
