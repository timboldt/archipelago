#[cfg(test)]
mod tests {
    use crate::components::*;
    use crate::island::IslandEconomy;
    use crate::resources::*;
    use crate::ship::spawn::STARTING_SIM_TICK;
    use crate::ship::{PlanningTuning, ShipState};
    use crate::simulation::SimulationPlugin;
    use bevy::prelude::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_two_island_trade_cycle() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(SimulationTick(STARTING_SIM_TICK));
        app.insert_resource(TimeScale(100.0));
        app.insert_resource(PlanningTuningRes(PlanningTuning::default()));
        app.insert_resource(FrameTimingsRes::default());
        app.insert_resource(WorldConfig {
            num_islands: 2,
            world_size: 1000.0,
            num_ships: 1,
            total_islands: 2,
            mainland_island_id: None,
        });
        app.init_resource::<Assets<ColorMaterial>>();
        app.init_resource::<Assets<Mesh>>();
        app.insert_resource(ShipMeshes {
            clipper: Handle::default(),
            freighter: Handle::default(),
            shorthaul: Handle::default(),
        });
        app.add_plugins(SimulationPlugin);

        let mut rng = StdRng::seed_from_u64(42);
        let pos0 = Vec2::new(0.0, 0.0);
        let pos1 = Vec2::new(100.0, 0.0);

        let (mut island0_eco, mut island0_ledger) = IslandEconomy::new(0, 2, &mut rng);
        let (mut island1_eco, mut island1_ledger) = IslandEconomy::new(1, 2, &mut rng);

        island0_eco.inventory[Commodity::Grain.idx()] = 1000.0;
        island0_eco.recompute_local_prices_with_ledger(STARTING_SIM_TICK, &mut island0_ledger);
        island1_eco.inventory[Commodity::Grain.idx()] = 0.0;
        island1_eco.recompute_local_prices_with_ledger(STARTING_SIM_TICK, &mut island1_ledger);

        island0_ledger[1] = island1_ledger[1];
        island0_ledger[1].tick_updated = STARTING_SIM_TICK;
        island1_ledger[0] = island0_ledger[0];
        island1_ledger[0].last_seen_tick = STARTING_SIM_TICK;

        let island0_entity = app
            .world_mut()
            .spawn((
                IslandMarker,
                IslandId(0),
                island0_eco,
                MarketLedger(island0_ledger),
                Position(pos0),
            ))
            .id();
        let island1_entity = app
            .world_mut()
            .spawn((
                IslandMarker,
                IslandId(1),
                island1_eco,
                MarketLedger(island1_ledger),
                Position(pos1),
            ))
            .id();

        app.insert_resource(IslandEntityMap(vec![island0_entity, island1_entity]));
        app.insert_resource(IslandPositions(vec![pos0, pos1]));
        app.insert_resource(RouteHistory {
            recent_route_departures: vec![vec![0.0; 2]; 2],
            route_departure_history: vec![vec![vec![0; 2]; 2]; 10],
            cursor: 0,
        });

        let mut ship = ShipState::new(pos0, 300.0, 2, 0);
        ship.cash = 5000.0;
        ship.cargo = Some((Commodity::Grain, 10.0));
        ship.purchase_price = 20.0;
        ship.docked_at = None;
        ship.target_island_id = Some(1);
        ship.target = pos1;
        ship.seed_initial_market_view(
            &[
                (
                    pos0,
                    app.world()
                        .get::<IslandEconomy>(island0_entity)
                        .unwrap()
                        .clone(),
                    app.world()
                        .get::<MarketLedger>(island0_entity)
                        .unwrap()
                        .0
                        .clone(),
                ),
                (
                    pos1,
                    app.world()
                        .get::<IslandEconomy>(island1_entity)
                        .unwrap()
                        .clone(),
                    app.world()
                        .get::<MarketLedger>(island1_entity)
                        .unwrap()
                        .0
                        .clone(),
                ),
            ],
            STARTING_SIM_TICK,
            0,
            &mut rng,
        );

        let (movement, trading, profile, ship_ledger) = ship.into_components();
        let ship_entity = app
            .world_mut()
            .spawn((
                ShipMarker,
                Position(pos0),
                movement,
                trading,
                profile,
                ship_ledger,
                Transform::from_translation(pos0.extend(1.0)),
            ))
            .id();

        let mut reached = false;
        let mut grain_sold = false;
        for _ in 0..500 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(16));
            app.update();
            let tra = app.world().get::<ShipTrading>(ship_entity).unwrap();
            if tra.docked_at == Some(1) {
                reached = true;
            }
            if tra
                .cargo
                .as_ref()
                .is_none_or(|(c, _)| *c != Commodity::Grain)
            {
                grain_sold = true;
            }
            if reached && grain_sold {
                break;
            }
        }

        assert!(reached, "Ship failed to reach destination");
        assert!(grain_sold, "Ship failed to sell grain");
    }
}
