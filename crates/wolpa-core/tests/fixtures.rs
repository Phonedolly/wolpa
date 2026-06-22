/// Fixture-based tests.
///
/// Fixture files are raw msgpack dumps of nvim --embed sessions.
/// Record once with `--ignored`, replay without nvim.
///
///     cargo test -- --ignored record_fixture
///     cargo test -- fixtures
///
/// Replay tests run as normal unit tests — no nvim binary required.
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

/// Encode a msgpack-RPC request array.
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

/// Record a fixture: spawn nvim, attach UI at 80×24, type "Hello" in insert mode,
/// then capture all raw msgpack output to a binary file.
#[tokio::test]
#[ignore = "requires nvim binary; generates fixture files"]
async fn record_basic_edit_fixture() {
    let mut child = Command::new("nvim")
        .args(["--embed", "--headless", "--clean"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("nvim should spawn");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    // Build attach request
    let opts = Value::Map(vec![
        (Value::String("ext_linegrid".into()), Value::Boolean(true)),
        (Value::String("ext_hlstate".into()), Value::Boolean(true)),
        (Value::String("ext_messages".into()), Value::Boolean(true)),
        (Value::String("ext_popupmenu".into()), Value::Boolean(true)),
        (Value::String("ext_tabline".into()), Value::Boolean(true)),
        (Value::String("ext_termcolors".into()), Value::Boolean(true)),
    ]);

    let mut msg_id = 0;

    // 1. nvim_ui_attach
    let req = encode_request(
        msg_id,
        "nvim_ui_attach",
        vec![Value::Integer(80.into()), Value::Integer(24.into()), opts],
    );
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();
    msg_id += 1;
    sleep(Duration::from_millis(200)).await;

    // 2. nvim_input "i" (enter insert mode)
    let req = encode_request(msg_id, "nvim_input", vec![Value::String("i".into())]);
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();
    msg_id += 1;
    sleep(Duration::from_millis(100)).await;

    // 3. nvim_input "Hello"
    let req = encode_request(msg_id, "nvim_input", vec![Value::String("Hello".into())]);
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();
    msg_id += 1;
    sleep(Duration::from_millis(100)).await;

    // 4. nvim_input "<Esc>"
    let req = encode_request(msg_id, "nvim_input", vec![Value::String("\x1b".into())]);
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();
    msg_id += 1;
    sleep(Duration::from_millis(100)).await;

    // 5. :quit to have nvim exit cleanly (flushes all pending output)
    let req = encode_request(msg_id, "nvim_command", vec![Value::String("qall!".into())]);
    stdin.write_all(&req).await.unwrap();
    stdin.flush().await.unwrap();

    // Wait for nvim to process and exit, then read all stdout
    sleep(Duration::from_millis(500)).await;

    // Read all available stdout data
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

    // Save fixture
    fs::create_dir_all(FIXTURES_DIR).expect("create fixtures dir");
    let path = Path::new(FIXTURES_DIR).join("basic_edit.bin");
    fs::write(&path, &raw).expect("write fixture");

    println!("Fixture saved: {} ({} bytes)", path.display(), raw.len());
    assert!(raw.len() > 0, "fixture should not be empty");
}

/// Replay the basic_edit fixture and verify grid state after attach + edit.
#[test]
fn replay_basic_edit_fixture() {
    let path = Path::new(FIXTURES_DIR).join("basic_edit.bin");
    if !path.exists() {
        eprintln!(
            "Fixture not found: {}. Run `cargo test -- --ignored record_basic_edit_fixture` first.",
            path.display()
        );
        return;
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
                    match event {
                        wolpa_core::event::UiEvent::Redraw(events) => {
                            grid.apply_batch(&events);
                            redraw_count += events.len();
                        }
                        _ => {}
                    }
                }
                msg_count += 1;
            }
            Err(rmpv::decode::Error::InvalidMarkerRead(_))
            | Err(rmpv::decode::Error::InvalidDataRead(_)) => break,
            Err(e) => panic!("decode error at message {}: {:?}", msg_count, e),
        }
    }

    println!(
        "Replayed {} messages ({} redraw events)",
        msg_count, redraw_count
    );

    let g = grid.grid(1).expect("grid 1 should exist after attach");
    println!("Grid size: {}x{}", g.width, g.height);

    // Print first 3 rows for debugging
    for r in 0..3.min(g.height) {
        let row_text: String = (0..g.width.min(20))
            .map(|c| {
                let cell = g.cell(r, c);
                if cell.text == " " {
                    '.'
                } else {
                    cell.text.chars().next().unwrap_or('?')
                }
            })
            .collect();
        println!("Row {}: {}", r, row_text);
    }
    assert!(
        g.width > 0 && g.height > 0,
        "grid should have non-zero size"
    );

    // The text "Hello" should appear somewhere on row 0
    let row0_text: String = (0..g.width).map(|c| g.cell(0, c).text.as_str()).collect();
    println!("Row 0: {:?}", row0_text);

    assert!(
        row0_text.contains("Hello"),
        "row 0 should contain 'Hello', got: {:?}",
        row0_text
    );
}
