//! The server end to end over HTTP: what it refuses, what it says of itself
//! and how a call crosses to the main thread.

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use futures::StreamExt as _;
use futures::executor::block_on;
use serde_json::{Value, json};

use super::bridge::{Bridge, Request};
use super::http::{self, Running};
use crate::features::edit_book::agent::Answer;

/// A server that answers `list_books` with one book.
fn started() -> Running {
    let (bridge, mut calls) = Bridge::new();
    std::thread::spawn(move || {
        block_on(async {
            while let Some(call) = calls.next().await {
                let answer = match call.request {
                    Request::ListBooks(_) => Ok(Answer::of(&json!({
                        "books": [{"book": "k", "title": "T", "client": call.client.name, "session": call.client.session}]
                    }))),
                    Request::DeleteBook { .. } => {
                        Err(crate::features::edit_book::agent::error::AgentError::Declined)
                    }
                    other => {
                        // What the tool asked for, by the name of its request
                        // and of its call on a book.
                        let named = format!("{other:?}");
                        let kind: String = named
                            .split(|c: char| !c.is_alphanumeric())
                            .filter(|word| !word.is_empty())
                            .take(1)
                            .collect();
                        let op = named
                            .split("op: ")
                            .nth(1)
                            .and_then(|rest| rest.split(|c: char| !c.is_alphanumeric()).next())
                            .unwrap_or_default()
                            .to_owned();
                        let mut answer = Answer::of(&json!({"request": kind, "op": op}));
                        answer.png = Some(vec![1, 2, 3]);
                        Ok(answer)
                    }
                };
                drop(call.reply.send(answer));
            }
        });
    });
    http::start(bridge).unwrap()
}

/// The address of a running server.
fn address(running: &Running) -> SocketAddr {
    running
        .url()
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .parse()
        .unwrap()
}

/// What came back: the status, the headers and the body.
struct Reply {
    status: u16,
    headers: String,
    body: String,
}

impl Reply {
    /// The JSON-RPC message of the body, plain or the first event of a
    /// stream.
    fn message(&self) -> Value {
        let text = self
            .body
            .lines()
            .find_map(|line| {
                line.strip_prefix("data: ")
                    .filter(|data| data.starts_with('{'))
            })
            .unwrap_or(&self.body);
        serde_json::from_str(text).unwrap_or(Value::Null)
    }

    fn header(&self, name: &str) -> Option<String> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }
}

/// Sends one request and reads the answer to its end.
fn post(at: SocketAddr, headers: &[(&str, String)], body: &Value) -> Reply {
    let body = body.to_string();
    let mut request = format!(
        "POST /mcp HTTP/1.1\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if !headers.iter().any(|(name, _)| *name == "Host") {
        write!(request, "Host: {at}\r\n").unwrap();
    }
    for (name, value) in headers {
        write!(request, "{name}: {value}\r\n").unwrap();
    }
    request.push_str("\r\n");
    request.push_str(&body);
    let mut stream = TcpStream::connect(at).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = String::new();
    // A stream of events ends when the server closes it.
    drop(stream.read_to_string(&mut answer));
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    Reply {
        status,
        headers: head.to_owned(),
        body: body.to_owned(),
    }
}

fn initialize() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "Test agent", "version": "1"}
    }})
}

/// Initializes a session and gives its id.
fn session(at: SocketAddr) -> (Reply, String) {
    let reply = post(at, &[], &initialize());
    let id = reply.header("mcp-session-id").unwrap_or_default();
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    post(
        at,
        &[
            ("Mcp-Session-Id", id.clone()),
            ("MCP-Protocol-Version", "2025-06-18".into()),
        ],
        &initialized,
    );
    (reply, id)
}

/// Calls a method in a session.
fn call(at: SocketAddr, id: &str, method: &str, params: &Value) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": 2, "method": method, "params": params});
    post(
        at,
        &[
            ("Mcp-Session-Id", id.to_owned()),
            ("MCP-Protocol-Version", "2025-06-18".into()),
        ],
        &message,
    )
    .message()
}

#[test]
fn the_server_listens_on_the_loopback_address_from_port_7341() {
    let running = started();
    let at = address(&running);
    assert!(at.ip().is_loopback());
    assert!(at.port() >= http::PORT);
    assert!(running.url().ends_with("/mcp"));
}

#[test]
fn a_second_server_takes_the_next_free_port() {
    let first = started();
    let second = started();
    assert_ne!(address(&first).port(), address(&second).port());
}

#[test]
fn a_program_of_this_computer_needs_nothing_more_than_the_address() {
    let running = started();
    let at = address(&running);
    assert_eq!(post(at, &[], &initialize()).status, 200);
}

#[test]
fn a_request_from_a_web_page_is_refused() {
    let running = started();
    let at = address(&running);
    let origin = ("Origin", "https://example.com".to_owned());
    assert_eq!(post(at, &[origin], &initialize()).status, 403);
}

#[test]
fn a_request_for_another_host_is_refused() {
    let running = started();
    let at = address(&running);
    let host = ("Host", "attacker.example:80".to_owned());
    assert_eq!(post(at, &[host], &initialize()).status, 403);
}

#[test]
fn the_server_names_itself_and_tells_the_agent_how_to_work() {
    let running = started();
    let (reply, id) = session(address(&running));
    let result = &reply.message()["result"];
    assert_eq!(result["serverInfo"]["name"], "mbook");
    assert!(
        result["instructions"]
            .as_str()
            .is_some_and(|text| text.contains("get_guide"))
    );
    assert_ne!(id, "");
}

#[test]
fn every_tool_of_the_reference_is_offered_with_its_schemas() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    let listed = call(at, &id, "tools/list", &json!({}));
    let tools = listed["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut names: Vec<_> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "add_chapter",
            "add_picture",
            "check_book",
            "create_book",
            "delete_book",
            "discard_changes",
            "edit_chapter",
            "export_book",
            "get_book",
            "get_guide",
            "import_book",
            "list_books",
            "move_chapter",
            "put_figure",
            "read_chapter",
            "read_figure",
            "remove_chapter",
            "remove_figure",
            "remove_picture",
            "render_figure",
            "save_book",
            "search_book",
            "set_contents",
            "set_landmarks",
            "set_meta",
            "show",
            "view_picture",
            "write_chapter",
        ]
    );
    for tool in &tools {
        assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
        assert_eq!(tool["outputSchema"]["type"], "object", "{}", tool["name"]);
    }
    let remove = tools
        .iter()
        .find(|tool| tool["name"] == "remove_chapter")
        .unwrap();
    assert_eq!(remove["annotations"]["destructiveHint"], true);
    let read = tools
        .iter()
        .find(|tool| tool["name"] == "read_chapter")
        .unwrap();
    assert_eq!(read["annotations"]["readOnlyHint"], true);
}

#[test]
fn the_skill_is_served_as_resources_and_prompts_are_offered() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    let resources = call(at, &id, "resources/list", &json!({}));
    let uris: Vec<_> = resources["result"]["resources"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|resource| resource["uri"].as_str().map(str::to_owned))
        .collect();
    assert!(uris.contains(&"skill://mbook-authoring/SKILL.md".to_owned()));
    let read = call(
        at,
        &id,
        "resources/read",
        &json!({"uri": "skill://mbook-authoring/references/figures.md"}),
    );
    assert!(
        read["result"]["contents"][0]["text"]
            .as_str()
            .is_some_and(|text| text.starts_with("# Diagrams and formulas"))
    );
    let prompts = call(at, &id, "prompts/list", &json!({}));
    let names: Vec<_> = prompts["result"]["prompts"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|prompt| prompt["name"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"write_chapter".to_owned()));
}

#[test]
fn a_guide_is_given_without_the_main_thread() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    let guide = call(
        at,
        &id,
        "tools/call",
        &json!({"name": "get_guide", "arguments": {"topic": "errors"}}),
    );
    assert_eq!(guide["result"]["structuredContent"]["topic"], "errors");
    let missing = call(
        at,
        &id,
        "tools/call",
        &json!({"name": "get_guide", "arguments": {"topic": "x"}}),
    );
    assert_eq!(missing["result"]["isError"], true);
    assert_eq!(
        missing["result"]["structuredContent"]["code"],
        "invalid_arguments"
    );
}

#[test]
fn a_call_crosses_to_the_main_thread_with_the_name_of_the_client() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    let listed = call(
        at,
        &id,
        "tools/call",
        &json!({"name": "list_books", "arguments": {}}),
    );
    let books = &listed["result"]["structuredContent"]["books"];
    assert_eq!(books[0]["title"], "T");
    assert_eq!(books[0]["client"], "Test agent");
    let refused = call(
        at,
        &id,
        "tools/call",
        &json!({"name": "delete_book", "arguments": {"book": "k"}}),
    );
    assert_eq!(refused["result"]["isError"], true);
    assert_eq!(refused["result"]["structuredContent"]["code"], "declined");
}

#[test]
fn a_figure_is_rendered_with_an_image() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    let drawn = call(
        at,
        &id,
        "tools/call",
        &json!({"name": "render_figure", "arguments": {"kind": "formula", "source": "x^2"}}),
    );
    let content = drawn["result"]["content"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        content
            .iter()
            .any(|block| block["type"] == "image" && block["mimeType"] == "image/png")
    );
}

#[test]
fn a_stopped_server_takes_no_more_connections() {
    let running = started();
    let at = address(&running);
    drop(running);
    let closed = (0..50).any(|_| {
        std::thread::sleep(Duration::from_millis(20));
        TcpStream::connect(at).is_err()
    });
    assert!(closed);
}

/// A call of every tool on the main thread, with the request and the call
/// on a book it must give.
fn tool_calls() -> Vec<(&'static str, Value, &'static str, &'static str)> {
    let mut calls = first_tool_calls();
    calls.extend(structure_tool_calls());
    calls
}

/// The calls of the tools of the library, reading and text.
fn first_tool_calls() -> Vec<(&'static str, Value, &'static str, &'static str)> {
    let book = "k";
    vec![
        (
            "create_book",
            json!({"title": "T", "language": "en"}),
            "CreateBook",
            "",
        ),
        ("import_book", json!({"path": "/x.epub"}), "ImportBook", ""),
        (
            "export_book",
            json!({"book": book, "format": "txt", "path": "/x.txt"}),
            "ExportBook",
            "",
        ),
        ("show", json!({"book": book}), "Show", ""),
        ("discard_changes", json!({"book": book}), "Discard", ""),
        (
            "view_picture",
            json!({"book": book, "path": "img/a.avif"}),
            "ViewPicture",
            "",
        ),
        (
            "add_picture",
            json!({"book": book, "source": {"svg": "<svg/>"}, "quality": 50}),
            "AddPicture",
            "",
        ),
        ("get_book", json!({"book": book}), "Book", "GetBook"),
        (
            "read_chapter",
            json!({"book": book, "chapter": "ch01.md"}),
            "Book",
            "ReadChapter",
        ),
        (
            "search_book",
            json!({"book": book, "query": "x"}),
            "Book",
            "Search",
        ),
        (
            "read_figure",
            json!({"book": book, "path": "dia/0001.mmd"}),
            "Book",
            "ReadFigure",
        ),
        (
            "add_chapter",
            json!({"book": book, "text": "# T"}),
            "Book",
            "AddChapter",
        ),
        (
            "edit_chapter",
            json!({"book": book, "chapter": "ch01.md", "edits": [{"old": "a", "new": "b"}]}),
            "Book",
            "EditChapter",
        ),
        (
            "write_chapter",
            json!({"book": book, "chapter": "ch01.md", "text": "x", "revision": "0"}),
            "Book",
            "WriteChapter",
        ),
        (
            "move_chapter",
            json!({"book": book, "chapter": "ch01.md", "position": 1}),
            "Book",
            "MoveChapter",
        ),
        (
            "remove_chapter",
            json!({"book": book, "chapter": "ch01.md"}),
            "Book",
            "RemoveChapter",
        ),
        (
            "remove_picture",
            json!({"book": book, "path": "img/a.avif"}),
            "Book",
            "RemovePicture",
        ),
        (
            "put_figure",
            json!({"book": book, "kind": "diagram", "source": "flowchart TD"}),
            "Book",
            "PutFigure",
        ),
        (
            "remove_figure",
            json!({"book": book, "path": "dia/0001.mmd"}),
            "Book",
            "RemoveFigure",
        ),
    ]
}

/// The calls of the tools of structure, checking and saving.
fn structure_tool_calls() -> Vec<(&'static str, Value, &'static str, &'static str)> {
    let book = "k";
    vec![
        (
            "set_meta",
            json!({"book": book, "publisher": null, "authors": ["A"]}),
            "Book",
            "SetMeta",
        ),
        (
            "set_contents",
            json!({"book": book, "automatic": true}),
            "Book",
            "SetContents",
        ),
        (
            "set_landmarks",
            json!({"book": book, "landmarks": []}),
            "Book",
            "SetLandmarks",
        ),
        ("check_book", json!({"book": book}), "Book", "Check"),
        ("save_book", json!({"book": book}), "Book", "Save"),
    ]
}

#[test]
fn every_tool_hands_its_request_to_the_main_thread() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    for (tool, arguments, request, op) in tool_calls() {
        let answer = call(
            at,
            &id,
            "tools/call",
            &json!({"name": tool, "arguments": arguments}),
        );
        let content = &answer["result"]["structuredContent"];
        assert_eq!(content["request"], request, "{tool}: {answer}");
        assert_eq!(content["op"], op, "{tool}");
        let blocks = answer["result"]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            blocks.iter().any(|block| block["type"] == "image"),
            "{tool}"
        );
    }
}

#[test]
fn every_prompt_starts_its_task_for_the_book_given() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    for name in ["write_chapter", "illustrate", "review_book"] {
        let named = call(
            at,
            &id,
            "prompts/get",
            &json!({"name": name, "arguments": {"book": "k", "about": "trains"}}),
        );
        let text = named["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(text.contains("`k`"), "{name}: {named}");
        let bare = call(at, &id, "prompts/get", &json!({"name": name}));
        let text = bare["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(text.contains("list_books"), "{name}: {bare}");
    }
}

#[test]
fn the_skill_is_listed_and_given_by_its_address() {
    let running = started();
    let at = address(&running);
    let (_, id) = session(at);
    let listed = call(at, &id, "skills/list", &json!({}));
    assert_eq!(
        listed["result"]["skills"][0]["frontmatter"]["name"], "mbook-authoring",
        "{listed}"
    );
    let uri = "skill://mbook-authoring/SKILL.md";
    let given = call(at, &id, "skills/get", &json!({"uri": uri}));
    assert_eq!(given["result"]["skill"]["uri"], uri, "{given}");
    let missing = call(
        at,
        &id,
        "skills/get",
        &json!({"uri": "skill://other/SKILL.md"}),
    );
    assert!(missing["error"].is_object(), "{missing}");
    let unread = call(
        at,
        &id,
        "resources/read",
        &json!({"uri": "skill://mbook-authoring/none.md"}),
    );
    assert!(unread["error"].is_object(), "{unread}");
}

mod following_the_setting {
    use gpui_kit::{AppContext as _, TestAppContext};

    use crate::app::agent::AgentServer;
    use crate::features::agents::{AgentStatus, Status};
    use crate::features::appearance::Appearance;
    use crate::testing::Fixture;

    fn status(cx: &mut TestAppContext) -> Status {
        cx.update(|cx| AgentStatus::global(cx).read(cx).status().clone())
    }

    /// Lets the thread of the server bind its port.
    fn settle(cx: &mut TestAppContext, until: impl Fn(&Status) -> bool) -> Status {
        for _ in 0..400 {
            cx.run_until_parked();
            let now = status(cx);
            if until(&now) {
                return now;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        status(cx)
    }

    #[gpui_kit::test]
    fn the_server_starts_and_stops_with_its_setting(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        cx.update(gpui_kit::init);
        fixture.install(cx);
        let appearance = cx.update(|cx| {
            let appearance = cx.new(Appearance::new);
            Appearance::install(appearance.clone(), cx);
            appearance
        });
        cx.run_until_parked();
        cx.update(AgentServer::install);
        assert_eq!(status(cx), Status::Off);
        appearance.update(cx, |appearance, cx| {
            appearance.update(|settings| settings.agents.serve = true, cx);
        });
        let running = settle(cx, |now| matches!(now, Status::Running(_)));
        assert!(matches!(running, Status::Running(url) if url.starts_with("http://127.0.0.1:")));
        appearance.update(cx, |appearance, cx| {
            appearance.update(|settings| settings.agents.serve = false, cx);
        });
        assert_eq!(settle(cx, |now| *now == Status::Off), Status::Off);
    }
}

#[test]
fn every_session_has_a_number_of_its_own_that_its_calls_carry() {
    let running = started();
    let at = address(&running);
    let session_of = |id: &str| {
        let listed = call(
            at,
            id,
            "tools/call",
            &json!({"name": "list_books", "arguments": {}}),
        );
        listed["result"]["structuredContent"]["books"][0]["session"].clone()
    };
    let (_, first) = session(at);
    let (_, second) = session(at);
    assert_eq!(session_of(&first), session_of(&first));
    assert_ne!(session_of(&first), session_of(&second));
}
