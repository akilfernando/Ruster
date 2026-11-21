# rot-mcp: Rot Engine for AI apps

`rot-mcp` is a stdio MCP server. It lets an AI app (Claude Code, Codex,
Cursor and others) read and change the scene open in the Rot Engine editor.
The creator sees every change as it happens, and AI changes share the
editor's Undo history.

## Connecting

In the editor, Window → AI Connection… → Connect an AI App… registers
`rot-mcp` with the AI apps it finds, with Undo. By hand, register `rot-mcp`
as a stdio MCP server with no arguments or environment variables (Codex:
forward `XDG_RUNTIME_DIR` and `XDG_CONFIG_HOME` with `env_vars`). While a scene is open, every editor listens at
`$XDG_RUNTIME_DIR/rot/<pid>.sock` and describes itself in `<pid>.json`
(project folder, scene, process start). `rot-mcp` picks the editor whose
project folder contains its working directory, else the only one open.

- With several open, `project_list` lists them and `project_connect` takes a
  project name, folder, scene file or ID.
- Given a folder or scene that is not open, `project_connect` starts Rot
  Engine on it.
- With none open, tools answer "Rot Engine is not running".
- The first time an app connects, the editor asks Allow Always, Allow Once
  or Deny. An app allowed always reconnects without asking, across restarts
  and rebuilds ([decision](../../docs/decisions/2026-11-17-trust-ai-apps-once.md)).
  The app is named by `rot-mcp --client NAME` or by its MCP
  `clientInfo.name`. Revoke it in File → Advanced → Rot connection….
- An allowed app may save the open scene and, in a Rot project, build into
  `<project>/Builds`.
- Turning off `allow_ai_connections` in the editor preferences stops
  listening.

Two older modes still work when their variables are set:
`ROT_MCP_PROJECT_REGISTRY` (a registry written by
`scripts/enroll_local_connector.py`) and `ROT_MCP_LOCATOR` (one editor's
`.json` record). In registry mode the `connection` value returned by
`project_connect` may still be passed; when left out, calls go to the
connected project.

## Working with the scene

- `scene_inspect` lists objects 16 at a time; follow `next_offset`.
- `scene_create`, `scene_transform`, `scene_color`, `scene_delete` and the
  other `scene_*` tools change the scene. They apply to the current scene.
  To make an edit fail if the scene changed since you read it, pass the
  `expected_session_token` and `expected_revision` from an inspect.
- Other `expected_*` arguments (asset, view, run and save tokens) are still
  required where a tool lists them; read them from the matching inspect tool.
- Decimal arguments accept JSON integers or decimal strings.
- When the creator pauses AI edits (the toolbar mushroom), changes wait
  inside the call until resumed, up to 50 seconds; reads still run.
- Pending transform and other ordinary field edits can commit before the AI
  edit. During a drag the AI edit waits up to a second. AI changes appear in
  the Edit menu as "Undo AI: Move Cube".
- In legacy scenes, `material_inspect` lists committed material settings and
  includes `pending_material` while the creator has an unapplied material edit.
  The summary gives the intent (`edit_shared`, `make_unique`, or `create`),
  `object_id`, `affected_object_count`, and the `material_id` for a shared edit
  or `source_material_id` for a copy. AI requests leave pending material edits
  in the Inspector. Material writes return `drafts_pending` until the creator
  applies or cancels the edit.
- In legacy scenes, `scene_inspect` reports `group_creation_blockers` for
  `scene_group_create`. Each entry includes a `code` and can include a named
  `example`. Codes are `input_actions`, `hud`, `script_sources`,
  `object_scripts`, and `application_script`. An empty list means the scene's
  content allows empty group creation. Session, revision, drafts and capacity
  checks still apply. A content refusal returns `invalid_arguments`.
- `viewport_screenshot` returns a PNG of the viewport, at most 512 pixels, in
  one call. `viewport_capture_start` and `viewport_capture_inspect` do the
  same in two steps. Use `viewport_capture_pick` with the capture ID and
  normalized coordinates to find the nearest pickable geometry in the
  authored scene. The query uses the captured viewport and its picking locks.
- `scene_undo` and `scene_redo` use the shared Undo history.
- Changes stay unsaved until `scene_save` (read its token with
  `scene_persistence_inspect`). Asset imports are saved to the project at
  once.

`tools/list` advertises the main project, object, component, asset, Play and
screenshot controls in under 15 KB. To find earlier or less common commands,
call `rot_tools` with `{"action":"list"}`. Use `action: "describe"` and a
`name` for the command's arguments. Existing command names remain callable
directly. Clients that only accept advertised tools can use `rot_tools` with
`action: "call"`, `name`, and `arguments` to run the same command.

Argument shapes and bounds are checked by `rot-mcp` before a call reaches
the editor. The editor handles approval and shared Undo for either route.

## Limits today

- One AI app per editor at a time. After a lost connection the adapter
  reconnects on the next call.
- The scene is limited to 256 objects and five lights.
- Linux only.

## Development

```sh
cargo test -p rot-mcp
cargo build -p rot-mcp
```
