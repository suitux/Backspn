//! `backspn-mcp`: a Model Context Protocol server over stdio that lets an AI
//! client plan and play sets in a running Backspn. Each tool call is passed
//! to the app through its local socket.

use std::io::{self, BufRead, Write};

use backspn_remote::catalog;
use backspn_remote::protocol::Request;
use backspn_remote::server::{Client, Connection};
use serde_json::{Value, json};

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const PROMPT: &str = "dj_session";
const NOT_RUNNING: &str = "Backspn is not running or AI control is off. Open Backspn and turn on \
     Preferences → AI control, then try again.";

const PARSE_ERROR: i64 = -32700;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Answers one JSON-RPC message; notifications get no answer.
fn handle(
    message: &Value,
    call: &mut dyn FnMut(Request) -> Result<Value, String>,
) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message["method"].as_str().unwrap_or_default();
    let params = &message["params"];
    let result = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
        "tools/call" => Ok(call_tool(params, call)),
        "prompts/list" => Ok(prompts()),
        "prompts/get" => prompt(params),
        _ => Err((METHOD_NOT_FOUND, format!("unknown method {method}"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
        }
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params["protocolVersion"].as_str().unwrap_or_default();
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|v| **v == asked)
        .unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}, "prompts": {}},
        "serverInfo": {"name": "backspn", "version": env!("CARGO_PKG_VERSION")},
        "instructions": catalog::INSTRUCTIONS
    })
}

fn tools() -> Value {
    let tools: Vec<Value> = catalog::tools()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema
            })
        })
        .collect();
    json!({"tools": tools})
}

fn call_tool(params: &Value, call: &mut dyn FnMut(Request) -> Result<Value, String>) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let outcome = catalog::request(name, params["arguments"].clone()).and_then(&mut *call);
    let (text, is_error) = match outcome {
        Ok(value) => (
            serde_json::to_string_pretty(&value).unwrap_or_default(),
            false,
        ),
        Err(message) => (message, true),
    };
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

const PROMPT_ARGUMENTS: [(&str, &str); 6] = [
    ("duration", "How long, e.g. \"60 min\""),
    ("genres", "Styles, e.g. \"deep house, progressive\""),
    ("tempo", "Tempo, e.g. \"120 to 126 BPM\""),
    ("energy", "Energy journey, e.g. \"3 up to 8, peak at 75 %\""),
    ("mood", "Mood or vibe, e.g. \"dark, hypnotic\""),
    (
        "notes",
        "Anything else: transitions, era, tracks to include…",
    ),
];

fn prompts() -> Value {
    let arguments: Vec<Value> = PROMPT_ARGUMENTS
        .iter()
        .map(|(name, description)| {
            json!({"name": name, "description": description, "required": false})
        })
        .collect();
    json!({"prompts": [{
        "name": PROMPT,
        "description": "Plan and play a DJ set in Backspn.",
        "arguments": arguments
    }]})
}

fn prompt(params: &Value) -> Result<Value, (i64, String)> {
    if params["name"] != PROMPT {
        return Err((INVALID_PARAMS, format!("unknown prompt {}", params["name"])));
    }
    let wishes: Vec<String> = PROMPT_ARGUMENTS
        .iter()
        .filter_map(|(name, _)| {
            params["arguments"][name]
                .as_str()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| format!("- {name}: {v}"))
        })
        .collect();
    let wishes = if wishes.is_empty() {
        "Ask me what I would like to hear.".to_owned()
    } else {
        wishes.join("\n")
    };
    let text = format!(
        "Play a DJ set for me in Backspn.\n\n{wishes}\n\nLook at the library first, plan the set, \
         show me the tracks and why they fit, then start it."
    );
    Ok(json!({
        "description": "Plan and play a DJ set in Backspn.",
        "messages": [{"role": "user", "content": {"type": "text", "text": text}}]
    }))
}

/// Sends a request to the running app, reading the connection file each
/// time so a restarted app is found again.
fn call_app(request: Request) -> Result<Value, String> {
    let data_dir = backspn_library::paths::data_dir();
    let connection = Connection::read(&data_dir).map_err(|_| NOT_RUNNING.to_owned())?;
    let mut client = Client::connect(&connection).map_err(|_| NOT_RUNNING.to_owned())?;
    client
        .call(request)
        .map_err(|e| format!("{NOT_RUNNING} ({e})"))?
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(&message, &mut call_app),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {"code": PARSE_ERROR, "message": e.to_string()}
            })),
        };
        if let Some(answer) = answer {
            writeln!(stdout, "{answer}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(message: &Value, call: &mut dyn FnMut(Request) -> Result<Value, String>) -> Value {
        handle(message, call).expect("an answer")
    }

    fn offline(_: Request) -> Result<Value, String> {
        Err(NOT_RUNNING.to_owned())
    }

    #[test]
    fn initialize_agrees_on_a_version_and_explains_the_flow() {
        let answer = ask(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                   "params": {"protocolVersion": "2025-03-26", "capabilities": {}}}),
            &mut offline,
        );
        assert_eq!(answer["id"], 1);
        assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
        assert!(
            answer["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("plan_set")
        );
        let newer = ask(
            &json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
                   "params": {"protocolVersion": "2099-01-01"}}),
            &mut offline,
        );
        assert_eq!(newer["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn notifications_get_no_answer() {
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert_eq!(handle(&note, &mut offline), None);
    }

    #[test]
    fn tools_come_from_the_catalog() {
        let answer = ask(
            &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
            &mut offline,
        );
        let tools = answer["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), catalog::tools().len());
        assert_eq!(tools[0]["name"], "library_overview");
        assert_eq!(tools[0]["inputSchema"]["type"], "object");
    }

    #[test]
    fn a_tool_call_reaches_the_app_and_comes_back_as_text() {
        let mut seen = Vec::new();
        let mut app = |request: Request| {
            seen.push(request);
            Ok(json!({"state": "playing"}))
        };
        let answer = ask(
            &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                   "params": {"name": "play", "arguments": {"deck": 1, "on": true}}}),
            &mut app,
        );
        assert_eq!(seen, [Request::Play { deck: 1, on: true }]);
        assert_eq!(answer["result"]["isError"], false);
        let text = answer["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("playing"));
    }

    #[test]
    fn failures_are_tool_errors_the_model_can_read() {
        let answer = ask(
            &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                   "params": {"name": "get_set_status"}}),
            &mut offline,
        );
        assert_eq!(answer["result"]["isError"], true);
        assert_eq!(answer["result"]["content"][0]["text"], NOT_RUNNING);
        let bad = ask(
            &json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                   "params": {"name": "play", "arguments": {}}}),
            &mut offline,
        );
        assert_eq!(bad["result"]["isError"], true);
        let unknown = ask(
            &json!({"jsonrpc": "2.0", "id": 7, "method": "resources/list"}),
            &mut offline,
        );
        assert_eq!(unknown["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn the_session_prompt_fills_in_the_wishes() {
        let list = ask(
            &json!({"jsonrpc": "2.0", "id": 8, "method": "prompts/list"}),
            &mut offline,
        );
        assert_eq!(list["result"]["prompts"][0]["name"], PROMPT);
        let answer = ask(
            &json!({"jsonrpc": "2.0", "id": 9, "method": "prompts/get",
                   "params": {"name": PROMPT, "arguments": {"duration": "45 min", "mood": " dark "}}}),
            &mut offline,
        );
        let text = answer["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("- duration: 45 min\n- mood: dark"), "{text}");
        let unknown = ask(
            &json!({"jsonrpc": "2.0", "id": 10, "method": "prompts/get", "params": {"name": "x"}}),
            &mut offline,
        );
        assert_eq!(unknown["error"]["code"], INVALID_PARAMS);
    }
}
