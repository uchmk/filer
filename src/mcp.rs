//! `kura mcp`: Claude Code reads what the window is showing (Q95, Q96).
//!
//! A windowed program has no standard input to speak MCP on, so the same exe
//! is the bridge:
//!
//! ```text
//! Claude Code ──stdio (JSON-RPC)── kura mcp ──socket / named pipe── kura (the window)
//! ```
//!
//! The window listens on a per-user door ([`address`]); the first window to
//! open it holds it, and the others try again every half minute, so closing
//! that one hands the door on. A request is answered on the UI thread, from
//! the state already in memory: nothing here waits on the disk there.
//!
//! The tools only read, `kura_reveal` aside, which moves the cursor and
//! changes no file. Writing tools are a later step, behind a confirm box in
//! the window (docs/llm-integration.md).

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use ito_ipc::{frame, Address, Listener};
use ito_mcp::serde_json::{json, Value};
use ito_mcp::{arg_str, Server, Tool};

/// Bumped when [`Request`] or [`Reply`] change shape, so a window and a
/// `kura mcp` from different versions say so instead of misreading.
const PROTO: u32 = 1;

/// How long `kura mcp` and the door wait for the window to answer. A window
/// that is busy for longer (a modal dialog on Windows) is reported, not waited for.
const ANSWER_WAIT: Duration = Duration::from_secs(5);

/// The door: `\\.\pipe\kura-<user>` on Windows, `$XDG_RUNTIME_DIR/kura/sock`
/// (or `/tmp/kura-<user>/sock`) elsewhere; `KURA_ADDRESS` names another.
pub fn address() -> Address {
    Address::per_user("kura", "KURA_ADDRESS")
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The folder, the cursor, the selection and the tabs, as JSON.
    State,
    /// Go to this absolute path's folder and put the cursor on it.
    Reveal(PathBuf),
}

/// What the window says: the tool's text, or why it could not.
pub type Reply = Result<String, String>;

/// One request handed to the UI thread, with where its answer goes.
pub struct Ask {
    pub request: Request,
    pub reply: Sender<Reply>,
}

/// Opens the door on a thread of its own and hands each request to the UI
/// thread through the returned channel, waking it with `ctx`.
pub fn listen(ctx: egui::Context) -> Receiver<Ask> {
    let (tx, rx) = bounded::<Ask>(16);
    let at = address();
    let _ = std::thread::Builder::new().name("kura-mcp".into()).spawn(move || {
        let listener = loop {
            match Listener::bind(&at) {
                Ok(l) => break l,
                // Another kura window has it. Try again later, so the door
                // moves on when that window closes.
                Err(e) if e.kind() == io::ErrorKind::AddrInUse => std::thread::sleep(Duration::from_secs(30)),
                // Nothing to report it to that anyone reads: the door is a
                // convenience, and the window works without it.
                Err(_) => return,
            }
        };
        while let Ok(conn) = listener.accept() {
            let tx = tx.clone();
            let ctx = ctx.clone();
            let _ = std::thread::Builder::new().name("kura-mcp-conn".into()).spawn(move || {
                let _ = serve_one(conn, &tx, &ctx);
            });
        }
    });
    rx
}

/// One connection: one request, one reply. A probe that connects and goes
/// away (another window checking whether the door is taken) ends here at the read.
fn serve_one(conn: ito_ipc::Conn, tx: &Sender<Ask>, ctx: &egui::Context) -> io::Result<()> {
    let (mut r, mut w) = conn.split()?;
    let (proto, request): (u32, Request) = frame::read(&mut r)?;
    let reply = if proto != PROTO {
        Err(format!("this kura window speaks version {PROTO} of the door and kura mcp spoke {proto}; run the same kura for both"))
    } else {
        answer(request, tx, ctx)
    };
    frame::write(&mut w, &reply)
}

fn answer(request: Request, tx: &Sender<Ask>, ctx: &egui::Context) -> Reply {
    // Checked here, off the UI thread: a path on a share that stopped
    // answering would hold the window.
    if let Request::Reveal(p) = &request {
        if !p.is_absolute() {
            return Err(format!("{} is not an absolute path", p.display()));
        }
        if std::fs::symlink_metadata(p).is_err() {
            return Err(format!("{} does not exist", p.display()));
        }
    }
    let (reply_tx, reply_rx) = bounded(1);
    tx.send_timeout(Ask { request, reply: reply_tx }, ANSWER_WAIT).map_err(|_| "the kura window is busy".to_owned())?;
    ctx.request_repaint();
    reply_rx.recv_timeout(ANSWER_WAIT).map_err(|_| "the kura window did not answer in time".to_owned())?
}

/// Asks the running window, for `kura mcp`.
fn ask(request: Request) -> Reply {
    let at = address();
    let conn = ito_ipc::connect(&at).map_err(|_| {
        "kura is not running (or its [mcp] enable is false in kura.toml); start kura and ask again".to_owned()
    })?;
    let (mut r, mut w) = conn.split().map_err(|e| e.to_string())?;
    frame::write(&mut w, &(PROTO, request)).map_err(|e| format!("could not reach the kura window: {e}"))?;
    frame::read::<_, Reply>(&mut r).map_err(|e| format!("the kura window did not answer: {e}"))?
}

/// The MCP server `kura mcp` runs on its standard input and output.
pub fn server() -> Server {
    Server::new("kura", env!("CARGO_PKG_VERSION"))
        .instructions(
            "kura is the file manager the user has open. kura_state says which folder they are \
             looking at, the file under the cursor and the files they selected; kura_reveal shows \
             them a file. Read the files themselves with your own tools.",
        )
        .tool(Tool::new(
            "kura_state",
            "What the user's kura window is showing: the current folder, the path under the cursor, \
             the selected paths and the open tabs. Use it when the user says \"this file\", \"here\" \
             or \"the selected files\".",
            |_| ask(Request::State),
        ))
        .tool(
            Tool::new(
                "kura_reveal",
                "Show a file or folder in the user's kura window: go to its folder and put the cursor \
                 on it. Changes no file.",
                |args| ask(Request::Reveal(PathBuf::from(arg_str(args, "path")?))),
            )
            .input(json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "An absolute path" } },
                "required": ["path"]
            })),
        )
}

/// `kura mcp`: serve until the client closes standard input.
pub fn run() -> ! {
    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    let code = match server().serve(stdin, stdout) {
        Ok(()) => 0,
        Err(_) => 1,
    };
    std::process::exit(code)
}

/// The window's answer to `kura_state`, as pretty JSON.
pub fn state_json(app: &crate::app::App, overlay: &str, view: &str) -> String {
    /// A selection of every file in a big folder is not worth the tokens.
    const MAX_SELECTED: usize = 500;
    let tab = app.tab();
    let selected: Vec<String> = tab.selected.iter().take(MAX_SELECTED).map(|p| p.display().to_string()).collect();
    let tabs: Vec<Value> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| json!({ "cwd": t.cwd.display().to_string(), "active": i == app.active }))
        .collect();
    let v = json!({
        "cwd": tab.cwd.display().to_string(),
        "hovered": tab.current.hovered().map(|e| e.path.display().to_string()),
        "selected": selected,
        "selected_count": tab.selected.len(),
        "tabs": tabs,
        "view": view,
        "overlay": overlay,
    });
    ito_mcp::serde_json::to_string_pretty(&v).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(server: &Server, name: &str, args: Value) -> Value {
        let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let out = server.handle(&line.to_string()).expect("a call is answered");
        ito_mcp::serde_json::from_str(&out).unwrap()
    }

    /// The tools Claude Code is offered, and that `kura_reveal` asks for a path.
    #[test]
    fn lists_the_two_tools() {
        let out = server().handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        let v: Value = ito_mcp::serde_json::from_str(&out).unwrap();
        let names: Vec<&str> = v["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["kura_state", "kura_reveal"]);
        assert_eq!(v["result"]["tools"][1]["inputSchema"]["required"][0], "path");
    }

    /// With no window behind the door, the tool says so as a tool error the
    /// model can read, not a protocol error.
    #[test]
    fn no_window_is_a_tool_error() {
        let dir = crate::util::test_dir("mcp-no-window");
        std::fs::create_dir_all(&dir).unwrap();
        // No other test reads it: `address` is called by the tools only.
        std::env::set_var("KURA_ADDRESS", dir.join("sock"));
        let v = call(&server(), "kura_state", json!({}));
        std::env::remove_var("KURA_ADDRESS");
        assert_eq!(v["result"]["isError"], true);
        assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains("kura is not running"));
    }

    /// The door end to end, without a window: a fake UI thread answers what
    /// comes down the channel, and a relative path never reaches it.
    #[cfg(unix)]
    #[test]
    fn the_door_carries_a_request_and_its_reply() {
        let dir = crate::util::test_dir("mcp-door");
        std::fs::create_dir_all(&dir).unwrap();
        let at = Address(dir.join("sock"));
        let listener = Listener::bind(&at).unwrap();
        let (tx, rx) = bounded::<Ask>(1);
        std::thread::spawn(move || {
            while let Ok(ask) = rx.recv() {
                let _ = ask.reply.send(Ok(format!("{:?}", ask.request)));
            }
        });
        let ctx = egui::Context::default();
        std::thread::spawn(move || {
            while let Ok(conn) = listener.accept() {
                let _ = serve_one(conn, &tx, &ctx);
            }
        });
        let send = |req: Request| -> Reply {
            let (mut r, mut w) = ito_ipc::connect(&at).unwrap().split().unwrap();
            frame::write(&mut w, &(PROTO, req)).unwrap();
            frame::read(&mut r).unwrap()
        };
        assert_eq!(send(Request::State), Ok("State".into()));
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(send(Request::Reveal(file.clone())), Ok(format!("Reveal({file:?})")));
        assert!(send(Request::Reveal("a.txt".into())).unwrap_err().contains("not an absolute path"));
        assert!(send(Request::Reveal(dir.join("gone"))).unwrap_err().contains("does not exist"));
    }
}
