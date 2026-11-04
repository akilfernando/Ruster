//! A small integer logistics experiment with explicit, atomic tick stepping.
//!
//! Each tick produces stock, then transfers available stock to a destination.
//! There is no wall clock, randomness, renderer, or background work.

use std::fmt;

use serde::{Deserialize, Serialize};

mod scenario;
pub use scenario::{Scenario, ScenarioError, HISTORY_LIMIT, SCENARIO_VERSION};

/// Fixed rates and starting inventory, in whole items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub initial_stock: u64,
    pub production_per_tick: u64,
    pub transfer_per_tick: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_stock: 0,
            production_per_tick: 3,
            transfer_per_tick: 2,
        }
    }
}

/// An owned snapshot; changing it cannot change the simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub tick: u64,
    pub stock: u64,
    pub delivered: u64,
    /// Cumulative production, excluding the initial stock.
    pub produced: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    ZeroTransferRate,
    Overflow(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTransferRate => write!(f, "transfer_per_tick must be greater than zero"),
            Self::Overflow(counter) => write!(f, "{counter} would overflow; tick was not applied"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct Simulation {
    config: Config,
    state: State,
}

impl Simulation {
    /// Zero production is allowed, but transport must have a positive capacity.
    pub fn new(config: Config) -> Result<Self, Error> {
        if config.transfer_per_tick == 0 {
            return Err(Error::ZeroTransferRate);
        }
        Ok(Self {
            config,
            state: State {
                stock: config.initial_stock,
                ..State::default()
            },
        })
    }

    /// Return to authored initial state; this is not arbitrary tick rollback.
    pub fn reset(&mut self) {
        self.state = State {
            stock: self.config.initial_stock,
            ..State::default()
        };
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Apply exactly one tick, returning the resulting snapshot.
    ///
    /// Production must fit in the source inventory before transport occurs.
    /// Every counter is checked; on any error the entire state is unchanged.
    pub fn step(&mut self) -> Result<State, Error> {
        let tick = self
            .state
            .tick
            .checked_add(1)
            .ok_or(Error::Overflow("tick"))?;
        let available = self
            .state
            .stock
            .checked_add(self.config.production_per_tick)
            .ok_or(Error::Overflow("stock"))?;
        let produced = self
            .state
            .produced
            .checked_add(self.config.production_per_tick)
            .ok_or(Error::Overflow("produced"))?;
        let transferred = available.min(self.config.transfer_per_tick);
        let delivered = self
            .state
            .delivered
            .checked_add(transferred)
            .ok_or(Error::Overflow("delivered"))?;
        self.state = State {
            tick,
            stock: available - transferred,
            delivered,
            produced,
        };
        Ok(self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_inputs_reproduce_every_tick() {
        let mut first = Simulation::new(Config::default()).unwrap();
        let mut second = Simulation::new(Config::default()).unwrap();
        for _ in 0..100 {
            assert_eq!(first.step(), second.step());
        }
        assert_eq!(
            first.state(),
            State {
                tick: 100,
                stock: 100,
                delivered: 200,
                produced: 300
            }
        );
    }

    #[test]
    fn stock_is_conserved_across_rates_and_initial_inventories() {
        for initial_stock in [0, 1, 20] {
            for production_per_tick in [0, 1, 7] {
                for transfer_per_tick in [1, 3, 10] {
                    let mut simulation = Simulation::new(Config {
                        initial_stock,
                        production_per_tick,
                        transfer_per_tick,
                    })
                    .unwrap();
                    let mut previous = simulation.state();
                    for tick in 1..=100 {
                        let state = simulation.step().unwrap();
                        assert_eq!(state.tick, tick);
                        assert_eq!(state.produced, tick * production_per_tick);
                        assert_eq!(
                            u128::from(state.stock) + u128::from(state.delivered),
                            u128::from(initial_stock) + u128::from(state.produced)
                        );
                        assert_eq!(
                            state.delivered - previous.delivered,
                            (previous.stock + production_per_tick).min(transfer_per_tick)
                        );
                        previous = state;
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_zero_transport_capacity() {
        assert_eq!(
            Simulation::new(Config {
                transfer_per_tick: 0,
                ..Config::default()
            })
            .unwrap_err(),
            Error::ZeroTransferRate
        );
    }

    #[test]
    fn every_overflow_leaves_the_entire_state_unchanged() {
        let cases = [
            (
                State {
                    tick: u64::MAX,
                    ..State::default()
                },
                "tick",
            ),
            (
                State {
                    stock: u64::MAX,
                    ..State::default()
                },
                "stock",
            ),
            (
                State {
                    produced: u64::MAX,
                    ..State::default()
                },
                "produced",
            ),
            (
                State {
                    delivered: u64::MAX,
                    ..State::default()
                },
                "delivered",
            ),
        ];
        for (state, counter) in cases {
            let mut simulation = Simulation {
                config: Config {
                    production_per_tick: 1,
                    transfer_per_tick: 1,
                    initial_stock: 0,
                },
                state,
            };
            for _ in 0..2 {
                assert_eq!(simulation.step(), Err(Error::Overflow(counter)));
                assert_eq!(simulation.state(), state);
            }
        }
    }

    #[test]
    fn cumulative_overflow_is_atomic_after_a_successful_tick() {
        let mut simulation = Simulation::new(Config {
            initial_stock: 0,
            production_per_tick: u64::MAX,
            transfer_per_tick: u64::MAX,
        })
        .unwrap();
        let before = simulation.step().unwrap();
        assert_eq!(before.delivered, u64::MAX);
        assert_eq!(simulation.step(), Err(Error::Overflow("produced")));
        assert_eq!(simulation.state(), before);
    }
}
