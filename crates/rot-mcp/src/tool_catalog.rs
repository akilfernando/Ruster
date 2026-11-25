//! Advertise the regular editing workflow. Keep additional operations callable
//! by name, including through clients that can only invoke advertised tools.
use super::{
    domain_error, exact_keys, object_schema, tool_definition, tool_definitions, tool_description,
    Server, SCENE_GUARDS,
};
use serde_json::{json, Value};

pub(super) fn advertised(name: &str) -> bool {
    matches!(
        name,
        "project_list"
            | "project_connect"
            | "project_disconnect"
            | "scene_inspect"
            | "scene_create"
            | "scene_group_create"
            | "scene_rename"
            | "scene_reparent"
            | "scene_transform"
            | "scene_transform_many"
            | "scene_delete"
            | "scene_delete_many"
            | "scene_undo"
            | "scene_redo"
            | "scene_save"
            | "scene_persistence_inspect"
            | "scene_components_inspect"
            | "scene_components_edit"
            | "viewport_screenshot"
            | "viewport_frame"
            | "asset_inspect"
            | "asset_import"
            | "asset_refresh"
            | "scene_model_place"
            | "scene_prefab"
            | "scene_mesh"
            | "behavior_runtime_inspect"
            | "behavior_runtime_state"
            | "behavior_runtime_reset"
            | "behavior_runtime_play"
            | "behavior_runtime_pause"
            | "behavior_runtime_step"
            | "behavior_runtime_stop"
            | "rot_tools"
    )
}

pub(super) fn definition() -> Value {
    json!({
        "name":"rot_tools",
        "inputSchema":object_schema(
            json!({
                "action":{"type":"string","enum":["list","describe","call"]},
                "name":{"type":"string"},
                "arguments":{"type":"object"}
            }),
            vec!["action"],
        )
    })
}

pub(super) fn handle(
    server: &mut Server,
    args: &Value,
    bridge: &mut impl FnMut(&str, &Value) -> Value,
) -> Value {
    let invalid = || {
        domain_error(
            "invalid_arguments",
            "Use list, describe with name, or call with name and arguments.",
        )
    };
    let Some(map) = args.as_object() else {
        return invalid();
    };
    if args["action"] == "list" && exact_keys(map, &["action"], &[]) {
        let entries: Vec<_> = tool_definitions()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .filter(|name| !advertised(name))
            .map(|name| json!({"name":name, "description":tool_description(name)}))
            .collect();
        return json!({"ok":true,"result":{"tools":entries}});
    }
    let Some(name) = args["name"].as_str() else {
        return invalid();
    };
    let Some(tool) = tool_definition(name).filter(|_| !advertised(name)) else {
        return domain_error(
            "unknown_additional_tool",
            "Use rot_tools list for additional commands or call an advertised tool directly.",
        );
    };
    match args["action"].as_str() {
        Some("describe") if exact_keys(map, &["action", "name"], &[]) => {
            let mut schema = tool["inputSchema"].clone();
            if let Some(required) = schema["required"].as_array_mut() {
                required.retain(|key| !SCENE_GUARDS.iter().any(|(guard, _)| key == *guard));
            }
            json!({"ok":true,"result":{
                "name":name,"description":tool_description(name),"inputSchema":schema,
                "annotations":tool["annotations"]
            }})
        }
        Some("call")
            if exact_keys(map, &["action", "name", "arguments"], &[])
                && args["arguments"].is_object() =>
        {
            server.call_tool(name, args["arguments"].clone(), bridge)
        }
        _ => invalid(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_commands_share_direct_call_validation_and_authority_path() {
        let mut server = super::super::tests::ready();
        let invoke = |server: &mut Server,
                      name,
                      arguments,
                      mut bridge: &mut dyn FnMut(&str, &Value) -> Value| {
            let request = json!({
                "jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":name,"arguments":arguments}
            });
            server
                .handle(&serde_json::to_vec(&request).unwrap(), &mut bridge)
                .unwrap()["result"]["structuredContent"]
                .clone()
        };
        let mut local = |_: &str, _: &Value| panic!("catalog must stay local");
        let list = invoke(
            &mut server,
            "rot_tools",
            json!({"action":"list"}),
            &mut local,
        );
        let names = list["result"]["tools"].as_array().unwrap();
        assert!(names.iter().any(|tool| tool["name"] == "scene_display_set"));
        assert!(names
            .iter()
            .all(|tool| !advertised(tool["name"].as_str().unwrap())));
        let description = invoke(
            &mut server,
            "rot_tools",
            json!({"action":"describe","name":"scene_display_set"}),
            &mut local,
        );
        assert_eq!(description["result"]["inputSchema"]["required"], json!([]));
        let primitive = invoke(
            &mut server,
            "rot_tools",
            json!({"action":"describe","name":"scene_primitive"}),
            &mut local,
        );
        assert!(primitive["result"]["description"]
            .as_str()
            .unwrap()
            .contains("radius 0.5 and height 2 along +Y"));
        let mut args = json!({
            "expected_session_token":"owner","expected_revision":2,
            "display":{"type":"legacy_sdr"}
        });
        let response = json!({"ok":false,"error":{"code":"approval_required"}});
        let mut bridge = |name: &str, forwarded: &Value| {
            assert_eq!(name, "scene_display_set");
            assert_eq!(forwarded["expected_revision"], "2");
            assert_eq!(forwarded["display"], json!({"type":"legacy_sdr"}));
            response.clone()
        };
        assert_eq!(
            invoke(&mut server, "scene_display_set", args.clone(), &mut bridge),
            response
        );
        assert_eq!(
            invoke(
                &mut server,
                "rot_tools",
                json!({"action":"call","name":"scene_display_set","arguments":args}),
                &mut bridge,
            ),
            response
        );
        args["display"] = json!({"type":"surface_hdr","exposure_stops":99});
        let refusal = invoke(
            &mut server,
            "rot_tools",
            json!({"action":"call","name":"scene_display_set","arguments":args}),
            &mut local,
        );
        assert_eq!(refusal["error"]["code"], "invalid_arguments");
    }
}
