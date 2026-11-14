//! Disposable startup grant, not production consent or a public command bypass.
use super::*;
pub struct Owner {
    editor: SceneEditor,
    nonce: String,
}
impl Owner {
    pub fn new() -> Self {
        let editor = SceneEditor::with_workspace(
            Session::new(Scene::demo()).unwrap(),
            Workspace::detached(),
        );
        Self {
            editor,
            nonce: format!("localhost-fixture-{}", std::process::id()),
        }
    }
    pub fn scene(&self) -> Scene {
        self.editor.session.scene().clone()
    }
    pub fn inspect(&mut self) -> Value {
        let mut result = self.editor.command(&self.nonce, "scene_inspect", json!({}));
        result["result"]["undo_len"] = json!(self.editor.session.undo_len());
        result
    }
    pub fn command(&mut self, request: &Value) -> Value {
        let op = request["operation"].as_str().unwrap_or("");
        if !matches!(op, "scene_transform" | "scene_undo") {
            return json!({"ok":false,"error":{"code":"fixture_operation_denied"}});
        }
        self.editor
            .command(&self.nonce, op, request["arguments"].clone())
    }
}
