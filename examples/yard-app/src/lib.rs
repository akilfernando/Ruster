//! Application-owned delivery rules. The engine kit supplies hosting, not goals.

use ruster_app::{egui, Application, Content, MeshMap, ResourceRoot, Step};
use ruster_core::{Config, Simulation, State};
use ruster_scene::{Object, Scene, Transform};

/// Change these defaults to make a different challenge, then run or package it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub production: u64,
    pub capacity: u64,
    pub goal: u64,
    pub budget: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            production: 3,
            capacity: 2,
            goal: 24,
            budget: 10,
        }
    }
}

impl Settings {
    pub fn validate(self) -> Result<(), String> {
        if self.production > 1_000
            || !(1..=1_000).contains(&self.capacity)
            || !(1..=1_000_000).contains(&self.goal)
            || !(1..=10_000).contains(&self.budget)
        {
            return Err("Use production 0–1000, capacity 1–1000, target 1–1000000 and time budget 1–10000 ticks.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    InProgress,
    Delivered,
    Missed,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::InProgress => "Delivery in progress",
            Self::Delivered => "Target delivered!",
            Self::Missed => "Time up — target missed",
        }
    }
}

/// Pure application state, usable without a desktop, renderer or agent service.
pub struct Delivery {
    settings: Settings,
    simulation: Simulation,
    last_shipment: u64,
}

impl Delivery {
    pub fn new(settings: Settings) -> Result<Self, String> {
        settings.validate()?;
        let simulation = Simulation::new(Config {
            initial_stock: 0,
            production_per_tick: settings.production,
            transfer_per_tick: settings.capacity,
        })
        .map_err(|e| e.to_string())?;
        Ok(Self {
            settings,
            simulation,
            last_shipment: 0,
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    pub fn state(&self) -> State {
        self.simulation.state()
    }

    pub fn last_shipment(&self) -> u64 {
        self.last_shipment
    }

    pub fn outcome(&self) -> Outcome {
        let state = self.state();
        // A delivery on the final tick counts: evaluate the target first.
        if state.delivered >= self.settings.goal {
            Outcome::Delivered
        } else if state.tick >= self.settings.budget {
            Outcome::Missed
        } else {
            Outcome::InProgress
        }
    }

    pub fn step(&mut self) -> Result<(), String> {
        if self.outcome() != Outcome::InProgress {
            return Ok(());
        }
        let before = self.state().delivered;
        let after = self.simulation.step().map_err(|e| e.to_string())?;
        self.last_shipment = after.delivered - before;
        Ok(())
    }
}

pub struct Yard {
    delivery: Delivery,
    draft: Settings,
    content: Content,
}

impl Yard {
    pub fn load(resources: &ResourceRoot, settings: Settings) -> Result<Self, String> {
        let mut meshes = MeshMap::new();
        meshes.insert("yard-truck".into(), resources.mesh("truck.obj")?);
        let delivery = Delivery::new(settings)?;
        let content = Content::new(scene_for(&delivery), meshes)?;
        Ok(Self {
            delivery,
            draft: settings,
            content,
        })
    }

    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }

    fn update_scene(&mut self) {
        *self.content.scene_mut() = scene_for(&self.delivery);
    }
}

impl Application for Yard {
    fn content(&self) -> &Content {
        &self.content
    }

    fn tick(&mut self, _step: Step) -> Result<(), String> {
        self.delivery.step()?;
        self.update_scene();
        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        // Validate/build first: an invalid setting preserves the current round.
        self.delivery = Delivery::new(self.draft)?;
        self.update_scene();
        Ok(())
    }

    fn completed(&self) -> bool {
        self.delivery.outcome() != Outcome::InProgress
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        let state = self.delivery.state();
        let settings = self.delivery.settings();
        ui.heading("Delivery challenge");
        ui.label(format!(
            "Deliver {} crates in {} ticks.",
            settings.goal, settings.budget
        ));
        ui.strong(self.delivery.outcome().label());
        ui.add(
            egui::ProgressBar::new((state.delivered as f32 / settings.goal as f32).min(1.0))
                .text(format!("{} / {} delivered", state.delivered, settings.goal)),
        );
        ui.label(format!(
            "Time remaining: {} ticks",
            settings.budget.saturating_sub(state.tick)
        ));
        ui.separator();
        ui.label(format!("Source stock: {} crates", state.stock));
        ui.label(format!(
            "Last shipment: {} / {} crates",
            self.delivery.last_shipment(),
            settings.capacity
        ));
        ui.label("Blue: source · Orange: truck · Green: destination");
        ui.small("Stack heights show source stock and delivery progress. The truck moves along the road as time passes; its load shows the last shipment.");
        ui.separator();
        ui.heading("Plan the next round");
        setting(
            ui,
            "Production / tick",
            &mut self.draft.production,
            0..=1_000,
        );
        setting(ui, "Truck capacity", &mut self.draft.capacity, 1..=1_000);
        setting(ui, "Delivery target", &mut self.draft.goal, 1..=1_000_000);
        setting(ui, "Time budget", &mut self.draft.budget, 1..=10_000);
        if self.draft != settings {
            ui.strong("Settings changed — press Reset to start the new round.");
            if ui.button("Discard setting changes").clicked() {
                self.draft = settings;
            }
        } else {
            ui.label("Change settings, then press Reset to apply them.");
        }
        ui.label("Try capacity 3 to complete the default challenge.");
        ui.separator();
        ui.small("Run/Pause: Space when the viewport has focus. Step advances one tick. Reset starts paused. Right-drag to orbit; scroll to zoom; use Fit to restore the view. Tab navigates controls.");
        Ok(())
    }
}

fn setting(ui: &mut egui::Ui, label: &str, value: &mut u64, range: std::ops::RangeInclusive<u64>) {
    ui.horizontal(|ui| {
        let label = ui.label(label);
        ui.add(egui::DragValue::new(value).range(range).speed(1.0))
            .labelled_by(label.id);
    });
}

fn block(id: &str, position: [f32; 3], scale: [f32; 3], color: [f32; 4]) -> Object {
    Object {
        material: None,
        name: None,
        id: id.into(),
        transform: Transform {
            translation: position,
            scale,
            ..Transform::default()
        },
        color,
        mesh: None,
    }
}

/// Presentation is derived from state; there is no render-owned simulation.
fn scene_for(delivery: &Delivery) -> Scene {
    let state = delivery.state();
    let settings = delivery.settings();
    let blue = [0.08, 0.48, 0.82, 1.0];
    let green = [0.1, 0.68, 0.27, 1.0];
    let source_height = 0.08 + (state.stock as f32 / settings.goal as f32).min(1.0) * 2.2;
    let delivered_height = 0.08 + (state.delivered as f32 / settings.goal as f32).min(1.0) * 2.2;
    let progress = state.tick as f32 / settings.budget as f32;
    let truck_x = -2.2 + 4.4 * progress;
    let mut truck = block(
        "truck",
        [truck_x, 0.3, 0.4],
        [0.7, 0.7, 0.7],
        [0.95, 0.36, 0.04, 1.0],
    );
    truck.mesh = Some("yard-truck".into());
    let cargo_height = 0.03 + delivery.last_shipment as f32 / settings.capacity as f32 * 0.5;
    Scene {
        display: Default::default(),
        lighting: Default::default(),
        materials: Default::default(),
        id: "delivery-yard".into(),
        revision: 0,
        objects: vec![
            block(
                "ground",
                [0.0, -0.2, 0.0],
                [10.0, 0.2, 5.0],
                [0.12, 0.16, 0.2, 1.0],
            ),
            block(
                "road",
                [0.0, -0.06, 0.4],
                [6.0, 0.08, 1.5],
                [0.24, 0.27, 0.3, 1.0],
            ),
            block("source-pad", [-3.5, 0.0, 0.0], [1.6, 0.2, 2.0], blue),
            block(
                "source-stock",
                [-3.5, 0.1 + source_height / 2.0, 0.0],
                [1.0, source_height, 1.0],
                blue,
            ),
            block("destination-pad", [3.5, 0.0, 0.0], [1.6, 0.2, 2.0], green),
            block(
                "delivered-stock",
                [3.5, 0.1 + delivered_height / 2.0, 0.0],
                [1.0, delivered_height, 1.0],
                green,
            ),
            truck,
            block(
                "cargo",
                [truck_x - 0.22, 0.74 + cargo_height / 2.0, 0.4],
                [0.6, cargo_height, 0.65],
                [0.95, 0.75, 0.18, 1.0],
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unique risk: app deadline/target ordering, terminal freeze and capacity
    // affect actual challenge outcome, beyond core throughput invariants.
    #[test]
    fn capacity_changes_outcome_and_final_tick_deliveries_count() {
        for (capacity, goal, tick, outcome, delivered) in [
            (2, 24, 10, Outcome::Missed, 20),
            (3, 24, 8, Outcome::Delivered, 24),
            (2, 20, 10, Outcome::Delivered, 20),
        ] {
            let mut app = Delivery::new(Settings {
                capacity,
                goal,
                ..Settings::default()
            })
            .unwrap();
            for _ in 0..20 {
                app.step().unwrap();
            }
            assert_eq!(
                (app.state().tick, app.outcome(), app.state().delivered),
                (tick, outcome, delivered)
            );
        }
    }

    // Unique risk: application graphics silently remain a static demo or allow
    // values outside scene limits when the editable challenge reaches bounds.
    #[test]
    fn presentation_tracks_stock_delivery_cargo_and_reset() {
        for settings in [
            Settings::default(),
            Settings {
                production: 1_000,
                capacity: 1,
                goal: 1_000_000,
                budget: 10_000,
            },
        ] {
            let mut delivery = Delivery::new(settings).unwrap();
            let before = scene_for(&delivery);
            for _ in 0..settings.budget {
                delivery.step().unwrap();
            }
            let after = scene_for(&delivery);
            before.validate().unwrap();
            after.validate().unwrap();
            assert_ne!(before.objects[3].transform, after.objects[3].transform);
            assert_ne!(before.objects[5].transform, after.objects[5].transform);
            assert_ne!(before.objects[6].transform, after.objects[6].transform);
            assert_ne!(before.objects[7].transform, after.objects[7].transform);
            assert_eq!(before, scene_for(&Delivery::new(settings).unwrap()));
        }
    }

    // Unique risk: pending settings leak into play, or Reset loses the previous
    // round when a code-authored setting fails validation.
    #[test]
    fn settings_apply_only_on_successful_reset_and_restore_scene() {
        let root =
            ResourceRoot::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"))
                .unwrap();
        let mut app = Yard::load(&root, Settings::default()).unwrap();
        let initial = app.content.scene().clone();
        app.draft.capacity = 3;
        app.tick(Step {
            tick: 1,
            dt: std::time::Duration::from_millis(500),
        })
        .unwrap();
        assert_eq!(app.delivery.state().delivered, Settings::default().capacity);
        app.reset().unwrap();
        assert_eq!(app.delivery.state(), State::default());
        assert_eq!(app.delivery.settings().capacity, 3);
        assert_eq!(app.content.scene(), &initial);
        app.tick(Step {
            tick: 1,
            dt: std::time::Duration::from_millis(500),
        })
        .unwrap();
        assert_eq!(app.delivery.state().delivered, 3);
        let state = app.delivery.state();
        let scene = app.content.scene().clone();
        app.draft.capacity = 0;
        assert!(app.reset().is_err());
        assert_eq!(app.delivery.state(), state);
        assert_eq!(app.content.scene(), &scene);
    }
}
