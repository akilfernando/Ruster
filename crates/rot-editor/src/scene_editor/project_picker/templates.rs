//! New Project templates. Each one builds an ordinary scene through the
//! normal `Session` edits, so a template scene is saved, reopened and edited
//! exactly like one a creator made by hand.
use rot_scene::{Object, PlayerAnchor, PlayerView, Scene, SceneError, Session, Transform};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::scene_editor) enum Template {
    /// A scene with only the default sun.
    Empty,
    /// Ground, sun, a cube and a game camera looking at it.
    #[default]
    Basic3d,
}

impl Template {
    pub(in crate::scene_editor) const ALL: [Self; 2] = [Self::Empty, Self::Basic3d];

    pub(in crate::scene_editor) fn label(self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::Basic3d => "Basic 3D",
        }
    }

    pub(in crate::scene_editor) fn tooltip(self) -> &'static str {
        match self {
            Self::Empty => "A scene with a sun and nothing else",
            Self::Basic3d => "Ground, sun, a cube and a game camera",
        }
    }

    /// The first scene of a new project made from this template.
    pub(in crate::scene_editor) fn save_new(self, path: &Path) -> Result<(), SceneError> {
        let mut session = Session::new(Scene::authored_empty("main"))?;
        match self {
            Self::Empty => {
                rot_scene::component_scene::ComponentScene::from_legacy(&session)?.save_new(path)
            }
            Self::Basic3d => {
                basic_3d(&mut session)?;
                // The starter content is the first saved state, with no Undo entries.
                Session::new(session.scene().clone())?.save(path)
            }
        }
    }
}

fn basic_3d(session: &mut Session) -> Result<(), SceneError> {
    let ids = session.allocate_object_ids(2)?;
    let mut ground = Object::cube(ids[0].clone(), Some("Ground".into()));
    ground.transform = Transform {
        translation: [0.0, -0.05, 0.0],
        scale: [20.0, 0.1, 20.0],
        ..Transform::default()
    };
    ground.color = [0.16, 0.18, 0.15, 1.0];
    let revision = session.create(session.scene().revision, ground)?;
    let mut cube = Object::cube(ids[1].clone(), Some("Cube".into()));
    cube.transform.translation = [0.0, 0.5, 0.0];
    cube.color = [0.8, 0.8, 0.8, 1.0];
    let mut revision = session.create(revision, cube)?;
    // Keep the sun's gizmo off the cube, as Unity places its Directional Light.
    let sun = session
        .scene()
        .lighting
        .lights()
        .next()
        .map(|(id, light)| (id.to_owned(), light.clone()));
    if let Some((id, mut light)) = sun {
        light.transform.translation = [-3.0, 3.0, -2.0];
        revision = session.set_light(revision, &id, light)?;
    }
    // The Game view looks at the cube from in front and a little above.
    session.set_player_view(
        revision,
        Some(PlayerView {
            anchor: PlayerAnchor::Fixed {
                position: [0.0, 0.5, 0.0],
            },
            yaw: 0.0,
            pitch: 0.35,
            distance: 7.0,
            vertical_fov_degrees: 60.0,
        }),
    )?;
    Ok(())
}
