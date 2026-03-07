//! Island economy update system.

use bevy::prelude::*;
use std::time::Instant;

use crate::components::MarketLedger;
use crate::island::IslandEconomy;
use crate::resources::{FrameTimingsRes, SimulationTick, TimeScale};

pub fn update_island_economy(
    mut query: Query<(&mut IslandEconomy, &mut MarketLedger)>,
    tick: Res<SimulationTick>,
    time_scale: Res<TimeScale>,
    time: Res<Time>,
    mut frame_timings: ResMut<FrameTimingsRes>,
) {
    let phase_start = Instant::now();
    let dt = time.delta_secs() * time_scale.0;

    query.par_iter_mut().for_each(|(mut economy, mut ledger)| {
        economy.produce_consume_and_price(dt, tick.0, &mut ledger.0);
    });

    frame_timings.accum_economy_ms += phase_start.elapsed().as_secs_f32() * 1000.0;
}
