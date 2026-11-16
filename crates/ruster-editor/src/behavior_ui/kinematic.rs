//! Explicit opt-in template for the bounded API7 sphere mover.
use super::*;

const TEMPLATE: &str = r#"return {
    update = function(transform, props, dt, host)
        -- Reset keeps the authored pose. Use saved Axis action IDs below.
        if dt == 0 then
            return {transform = transform, state = {contacts = 0, outcome = "clear"}}
        end
        local dx, dz = 0, 0
        if props.axis_x ~= "" then dx = host.axis(props.axis_x) * props.speed * dt end
        if props.axis_z ~= "" then dz = host.axis(props.axis_z) * props.speed * dt end
        local motion = host.move_and_slide({dx, 0, dz})
        transform.translation = motion.suggested_translation
        return {transform = transform,
                state = {contacts = #motion.contacts, outcome = motion.outcome}}
    end
}
"#;

impl BehaviorUi {
    pub(super) fn create_kinematic(
        &mut self,
        session: &mut Session,
        object_id: String,
    ) -> Result<(), String> {
        if !session.scene().hierarchy.is_empty() {
            return Err(
                "Kinematic movement requires a flat scene; Groups are not supported in this slice."
                    .into(),
            );
        }
        let object = session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .ok_or("Select a geometry object for a kinematic mover")?;
        let collider = object
            .collider
            .ok_or("Add an enabled Blocker Sphere collider before creating a kinematic mover")?;
        if !collider.enabled
            || collider.role != ruster_scene::ColliderRole::Blocker
            || !matches!(collider.shape, ruster_scene::ColliderShape::Sphere { .. })
        {
            return Err("Kinematic movement requires an enabled Blocker Sphere collider.".into());
        }
        collider
            .resolve(&object.transform)
            .map_err(|error| error.to_string())?;
        for binding in session
            .authoring()
            .bindings
            .values()
            .filter(|binding| binding.enabled)
        {
            if session.source(&binding.source_id).is_some_and(|source| {
                source.host_api_version == ruster_behavior::KINEMATIC_HOST_API_VERSION
            }) {
                return Err("Only one enabled kinematic mover is supported; disable the existing mover first.".into());
            }
            if let Some(id) = binding.object_id().filter(|id| *id != object_id) {
                if session.scene().objects.iter().any(|object| {
                    object.id == id
                        && object.collider.is_some_and(|collider| {
                            collider.enabled && collider.role == ruster_scene::ColliderRole::Blocker
                        })
                }) {
                    return Err(format!("Blocker {id} must stay stationary; disable its behavior before adding this mover."));
                }
            }
        }
        let source = session
            .allocate_source_ids(1)
            .map_err(|error| error.to_string())?
            .remove(0);
        let binding = session
            .allocate_binding_ids(1)
            .map_err(|error| error.to_string())?
            .remove(0);
        let api = ruster_behavior::KINEMATIC_HOST_API_VERSION;
        session
            .create_behavior(
                session.scene().revision,
                &source,
                BehaviorSource {
                    label: Some("Kinematic mover".into()),
                    language_id: ruster_behavior::LANGUAGE_ID.into(),
                    host_api_version: api,
                    text: annotated_template(&source, api, TEMPLATE)?,
                },
                &binding,
                BehaviorBinding {
                    target: BindingTarget::Object(object_id),
                    source_id: source.clone(),
                    enabled: true,
                    commands: Default::default(),
                    properties: BTreeMap::from([
                        ("speed".into(), PropertyValue::Number(2.)),
                        ("axis_x".into(), PropertyValue::Text(String::new())),
                        ("axis_z".into(), PropertyValue::Text(String::new())),
                    ]),
                    state_defaults: BTreeMap::from([
                        ("contacts".into(), ruster_scene::StateValue::Number(0.)),
                        (
                            "outcome".into(),
                            ruster_scene::StateValue::Text("clear".into()),
                        ),
                    ]),
                },
            )
            .map_err(|error| error.to_string())?;
        self.load_source(session, &source);
        self.source_open = true;
        self.source_focus = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn template_creation_is_atomic_and_runs_reset_and_neutral_step() {
        let mut scene = ruster_scene::Scene::empty("mover");
        let mut object = ruster_scene::Object::cube("actor", None);
        scene.objects.push(object.clone());
        let mut session = Session::new(scene.clone()).unwrap();
        let mut ui = BehaviorUi::default();
        assert!(ui.create_kinematic(&mut session, "actor".into()).is_err());
        assert_eq!(session.undo_len(), 0);
        object.collider = Some(ruster_scene::Collider {
            enabled: true,
            shape: ruster_scene::ColliderShape::Sphere { radius: 0.25 },
            ..Default::default()
        });
        scene.objects[0] = object;
        let mut session = Session::new(scene).unwrap();
        ui.create_kinematic(&mut session, "actor".into()).unwrap();
        assert_eq!(session.undo_len(), 1);
        let mut engine = ruster_behavior::Engine::new(Default::default()).unwrap();
        let mut runtime = engine.reset(session.scene(), session.authoring()).unwrap();
        engine.step(&mut runtime).unwrap();
        assert_eq!(
            runtime.scene().objects[0].transform,
            session.scene().objects[0].transform
        );
        assert!(ui.create_kinematic(&mut session, "actor".into()).is_err());
        assert_eq!(session.undo_len(), 1);
        session.undo(session.scene().revision).unwrap();
        assert!(session.authoring().bindings.is_empty());
        assert!(session.authoring().sources.is_empty());
    }
}
