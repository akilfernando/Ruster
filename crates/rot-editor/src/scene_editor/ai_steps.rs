//! Recent changes made by an AI app, named for the Edit menu ("Undo AI: Move
//! Cube") and for the AI activity list.
use super::*;
use std::collections::VecDeque;

const KEPT: usize = 16;

pub(super) struct AiStep {
    pub(super) label: String,
    /// The scene revision this change produced.
    pub(super) revision: u64,
}

#[derive(Default)]
pub(super) struct AiSteps {
    steps: VecDeque<AiStep>,
}

impl AiSteps {
    pub(super) fn record(&mut self, label: String, revision: u64) {
        self.steps.push_back(AiStep { label, revision });
        if self.steps.len() > KEPT {
            self.steps.pop_front();
        }
    }
    /// The AI step that made the current scene, if the last change was one.
    pub(super) fn top(&self, revision: u64) -> Option<&str> {
        self.steps
            .back()
            .filter(|step| step.revision == revision)
            .map(|step| step.label.as_str())
    }
    /// Newest first.
    #[allow(dead_code)] // the AI activity list (A-05) reads this
    pub(super) fn recent(&self) -> impl Iterator<Item = &AiStep> {
        self.steps.iter().rev()
    }
}

/// "AI: Move Cube" for a tool call, named from the scene before it runs.
/// None for calls that change nothing worth naming (Undo and Redo).
pub(super) fn label(op: &str, args: &Value, scene: &Scene) -> Option<String> {
    let verb = match op {
        "scene_undo" | "scene_redo" => return None,
        "scene_transform" | "scene_transform_many" => "Move",
        "scene_create" => "Create",
        "scene_delete" | "scene_delete_many" => "Delete",
        "scene_rename" => "Rename",
        "scene_duplicate" | "scene_duplicate_many" => "Duplicate",
        "scene_color" => "Color",
        "scene_primitive" => "Change Shape",
        "scene_mesh" => "Set Mesh",
        "scene_material_assign" => "Assign Material",
        "scene_material_set" | "scene_material_copy" | "scene_material_remove" => "Edit Material",
        "scene_light_create" => "Create Light",
        "scene_light_set" => "Edit Light",
        "scene_ambient_set" => "Ambient Light",
        "scene_display_set" => "Display Settings",
        "scene_collider_set" => "Edit Collider",
        "scene_group_create" => "Create Group",
        "scene_reparent" => "Reparent",
        "scene_model_place" => "Place Model",
        "scene_player_view_set" => "Game Camera",
        "scene_animation_set" => "Animation",
        op if op.starts_with("behavior_") => "Edit Script",
        op if op.starts_with("hud_") => "Edit HUD",
        _ => "Edit",
    };
    let count = ["transforms", "objects", "object_ids"]
        .iter()
        .find_map(|field| args[*field].as_array().map(Vec::len));
    let subject = match count {
        Some(1) | None => args["object_id"]
            .as_str()
            .or_else(|| args["transforms"][0]["object_id"].as_str())
            .or_else(|| args["objects"][0]["object_id"].as_str())
            .or_else(|| args["object_ids"][0].as_str())
            .map(|id| {
                if op == "scene_create" {
                    args["name"].as_str().unwrap_or(id).to_owned()
                } else {
                    scene.target_display_name(id).unwrap_or(id).to_owned()
                }
            }),
        Some(n) => Some(format!("{n} Objects")),
    };
    Some(match subject {
        Some(subject) => format!("AI: {verb} {subject}"),
        None => format!("AI: {verb}"),
    })
}

impl SceneEditor {
    /// The Edit menu's Undo and Redo text, naming an AI change on top.
    pub(super) fn history_label(&self, action: Action) -> String {
        match (action, self.ai_steps.top(self.session.scene().revision)) {
            (Action::Undo, Some(label)) => format!("Undo {label}"),
            _ => action.label().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_changes_are_named_and_shown_on_undo() {
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let scene = editor.session.scene().clone();
        let id = scene.objects[0].id.clone();
        let name = scene.target_display_name(&id).unwrap().to_owned();
        let args = json!({"object_id":id,"transform":{}});
        assert_eq!(
            label("scene_transform", &args, &scene).unwrap(),
            format!("AI: Move {name}")
        );
        assert_eq!(
            label(
                "scene_delete_many",
                &json!({"object_ids":["a","b"]}),
                &scene
            )
            .unwrap(),
            "AI: Delete 2 Objects"
        );
        assert!(label("scene_undo", &json!({}), &scene).is_none());

        let revision = editor.session.scene().revision;
        editor.ai_steps.record("AI: Move Cube".into(), revision);
        assert_eq!(editor.history_label(Action::Undo), "Undo AI: Move Cube");
        editor.ai_steps.record("AI: Move Cube".into(), revision + 1);
        assert_eq!(editor.history_label(Action::Undo), Action::Undo.label());
    }
}
