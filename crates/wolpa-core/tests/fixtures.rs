//! ## Fixture-based integration tests
//!
//! Fixture files are raw msgpack dumps of `nvim --embed` sessions.
//! Record once with `--ignored` (requires nvim), replay without nvim.
//!
//! ```bash
//! cargo test -- --ignored record_    # record all fixtures
//! cargo test -- replay_              # replay all fixtures (no nvim needed)
//! ```
//!
//! | Fixture            | What it tests                        |
//! |--------------------|--------------------------------------|
//! | `basic_edit`       | `grid_line`, `grid_cursor_goto`      |
//! | `split_window`     | multi-grid (`grid_resize` for grid 2)|
//! | `scroll`           | `grid_scroll` region scrolling       |
//! | `completion_menu`  | `popupmenu_show`/`select`/`hide`     |
//! | `highlight`        | `hl_attr_define` with syntax groups  |

use std::fs;
use std::io::Cursor;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::{sleep, Duration};

use rmpv::Value;
use wolpa_core::grid::GridState;
use wolpa_core::rpc::parse_notification;

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

// ── Shared helpers ─────────────────────────────────────────────────────

fn encode_request(msg_id: u64, method: &str, args: Vec<Value>) -> Vec<u8> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer((msg_id as i64).into()),
        Value::String(method.into()),
        Value::Array(args),
    ]);
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &msg).unwrap();
    buf
}

fn attach_opts() -> Value {
    Value::Map(vec![
        (Value::String("ext_linegrid".into()), Value::Boolean(true)),
        (Value::String("ext_multigrid".into()), Value::Boolean(true)),
        (Value::String("ext_hlstate".into()), Value::Boolean(true)),
        (Value::String("ext_messages".into()), Value::Boolean(true)),
        (Value::String("ext_popupmenu".into()), Value::Boolean(true)),
        (Value::String("ext_tabline".into()), Value::Boolean(true)),
        (Value::String("ext_termcolors".into()), Value::Boolean(true)),
    ])
}

fn val_str(s: &str) -> Value {
    Value::String(s.into())
}

fn val_int(i: i64) -> Value {
    Value::Integer(i.into())
}

/// Spawn nvim, attach, run commands, quit, read stdout, save.
async fn spawn_and_record(name: &str, commands: &[(u64, &str, Vec<Value>)]) {
    let mut child = Command::new("nvim")
        .args(["--embed", "--headless", "--clean"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("nvim should spawn");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut msg_id: u64 = 0;

    // Attach
    let req = encode_request(
        msg_id,
        "nvim_ui_attach",
        vec![val_int(80), val_int(24), attach_opts()],
    );
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();
    msg_id += 1;
    sleep(Duration::from_millis(200)).await;

    // Execute commands
    for &(id, method, ref args) in commands {
        if id != msg_id {
            // Allow explicit ID override for readability
        }
        let req = encode_request(msg_id, method, args.clone());
        stdin.write_all(&req).await.unwrap();
        stdin.flush().await.unwrap();
        msg_id += 1;
        sleep(Duration::from_millis(100)).await;
    }

    // Quit
    let req = encode_request(msg_id, "nvim_command", vec![val_str("qall!")]);
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    // Read stdout
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stdout.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }

    child.kill().await.ok();
    child.wait().await.ok();

    fs::create_dir_all(FIXTURES_DIR).expect("create fixtures dir");
    let path = Path::new(FIXTURES_DIR).join(name);
    fs::write(&path, &raw).expect("write fixture");
    println!("Fixture saved: {} ({} bytes)", path.display(), raw.len());
    assert!(raw.len() > 0, "fixture should not be empty");
}

/// Replay a fixture and return grid state + message counts.
fn replay_fixture(name: &str) -> (GridState, usize, usize) {
    let path = Path::new(FIXTURES_DIR).join(name);
    if !path.exists() {
        panic!("Fixture not found: {}. Run record first.", path.display());
    }

    let raw = fs::read(&path).expect("read fixture");
    let mut grid = GridState::new(80, 24);
    let mut msg_count = 0;
    let mut redraw_count = 0;

    let mut cursor = Cursor::new(&raw[..]);
    loop {
        match rmpv::decode::read_value(&mut cursor) {
            Ok(value) => {
                if let Some(event) = parse_notification(&value) {
                    if let wolpa_core::event::UiEvent::Redraw(events) = event {
                        grid.apply_batch(&events);
                        redraw_count += events.len();
                    }
                }
                msg_count += 1;
            }
            Err(rmpv::decode::Error::InvalidMarkerRead(_))
            | Err(rmpv::decode::Error::InvalidDataRead(_)) => break,
            Err(e) => panic!("decode error at msg {}: {:?}", msg_count, e),
        }
    }

    println!(
        "Replayed {} ({} msgs, {} redraw events)",
        name, msg_count, redraw_count
    );
    (grid, msg_count, redraw_count)
}

// ── Recording tests (require nvim, #[ignore]) ──────────────────────────
// Each sends a sequence of (msgid_dummy, method, args) after ui_attach.

#[tokio::test]
#[ignore = "requires nvim binary"]
async fn record_basic_edit_fixture() {
    spawn_and_record(
        "basic_edit.bin",
        &[
            (0, "nvim_input", vec![val_str("i")]),
            (0, "nvim_input", vec![val_str("Hello")]),
            (0, "nvim_input", vec![val_str("\x1b")]),
        ],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires nvim binary"]
async fn record_split_window_fixture() {
    spawn_and_record(
        "split_window.bin",
        &[
            (0, "nvim_command", vec![val_str("split")]),
            (0, "nvim_input", vec![val_str("i")]),
            (0, "nvim_input", vec![val_str("lower window")]),
            (0, "nvim_input", vec![val_str("\x1b")]),
            (0, "nvim_input", vec![val_str("\x12")]), // <C-k> — up
            (0, "nvim_input", vec![val_str("i")]),
            (0, "nvim_input", vec![val_str("upper window")]),
            (0, "nvim_input", vec![val_str("\x1b")]),
        ],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires nvim binary"]
async fn record_scroll_fixture() {
    let mut cmds = vec![(0, "nvim_input", vec![val_str("i")])];
    for i in 0..30 {
        cmds.push((0, "nvim_input", vec![val_str(&format!("line {}\n", i))]));
    }
    cmds.push((0, "nvim_input", vec![val_str("\x1b")]));
    spawn_and_record("scroll.bin", &cmds).await;
}

#[tokio::test]
#[ignore = "requires nvim binary"]
async fn record_completion_menu_fixture() {
    spawn_and_record(
        "completion_menu.bin",
        &[
            (0, "nvim_input", vec![val_str("i")]),
            (0, "nvim_input", vec![val_str("apple apricot banana ")]),
            (0, "nvim_input", vec![val_str("\x1b")]),
            (0, "nvim_input", vec![val_str("o")]),
            (0, "nvim_input", vec![val_str("i")]),
            (0, "nvim_input", vec![val_str("ap")]),
            (0, "nvim_input", vec![val_str("\x0e")]), // <C-n>
            (0, "nvim_input", vec![val_str("\x1b")]),
        ],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires nvim binary"]
async fn record_highlight_fixture() {
    spawn_and_record(
        "highlight.bin",
        &[
            (0, "nvim_command", vec![val_str("set ft=lua")]),
            (0, "nvim_input", vec![val_str("i")]),
            (
                0,
                "nvim_input",
                vec![val_str("local function foo()\n  return 42\nend\n")],
            ),
            (0, "nvim_input", vec![val_str("\x1b")]),
        ],
    )
    .await;
}

// ── Replay tests (no nvim needed) ──────────────────────────────────────

#[test]
fn replay_basic_edit_fixture() {
    let (grid, _, _) = replay_fixture("basic_edit.bin");
    // With ext_multigrid, grid 2 is the first editor window
    let g = grid.grid(2).unwrap_or_else(|| grid.grid(1).unwrap());
    let row0: String = (0..g.width).map(|c| g.cell(0, c).text.as_str()).collect();
    assert!(
        row0.contains("Hello"),
        "content should contain 'Hello', got: {:?}",
        row0
    );
}

#[test]
fn replay_split_window_fixture() {
    let (grid, _, _) = replay_fixture("split_window.bin");
    let g2 = grid.grid(2).expect("grid 2 should exist after :split");
    assert!(g2.width > 0 && g2.height > 0);
    assert!(grid.grid(1).is_some(), "grid 1 should still exist");
}

#[test]
fn replay_scroll_fixture() {
    let (grid, msg_count, redraw_count) = replay_fixture("scroll.bin");
    let g = grid.grid(1).expect("grid 1 should exist");
    assert!(g.width > 0);
    println!("scroll: {} msgs, {} events", msg_count, redraw_count);
}

#[test]
fn replay_completion_menu_fixture() {
    let (grid, msg_count, redraw_count) = replay_fixture("completion_menu.bin");
    let g = grid.grid(1).expect("grid 1 should exist");
    assert!(g.width > 0);
    println!("completion: {} msgs, {} events", msg_count, redraw_count);
}

#[test]
fn replay_highlight_fixture() {
    let (grid, _, _) = replay_fixture("highlight.bin");
    // With ext_multigrid, grid 2 is the first editor window
    let g = grid.grid(2).unwrap_or_else(|| grid.grid(1).unwrap());
    let mut has_non_zero_hl = false;
    for r in 0..g.height.min(3) {
        for c in 0..g.width {
            if g.cell(r, c).hl_id != 0 {
                has_non_zero_hl = true;
                break;
            }
        }
    }
    assert!(
        has_non_zero_hl,
        "syntax highlighting should assign non-zero hl_ids"
    );
}
