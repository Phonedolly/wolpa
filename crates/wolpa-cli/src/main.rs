//! ## wolpa-cli — Debug CLI tool
//!
//! A command-line tool for testing `wolpa-core` without the macOS app.
//! Spawns `nvim --embed`, attaches as a UI client, sends commands/input,
//! and prints grid state and redraw events to the terminal.
//!
//! ### Usage
//!
//! ```bash
//! cargo run -p wolpa-cli                 # Demo: attach, edit, print grid
//! cargo run -p wolpa-cli -- --exec "vs"   # Run an Ex command
//! cargo run -p wolpa-cli -- --input "iHello<Esc>"  # Send keystrokes
//! cargo run -p wolpa-cli -- --dump         # Just attach and dump grid
//! ```

use std::io::Write as _;
use wolpa_core::grid::GridState;
use wolpa_core::rpc::RpcClient;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = if args.len() > 1 { &args[1] } else { "demo" };
    let extra = args.get(2).cloned();

    match mode {
        "--exec" => {
            run_exec(extra.as_deref().unwrap_or("ls")).await;
        }
        "--input" => {
            run_input(extra.as_deref().unwrap_or("iHello\x1b")).await;
        }
        "--dump" => {
            run_dump().await;
        }
        _ => {
            run_demo().await;
        }
    }
}

/// Demo: attach, type "Hello World", print grid.
async fn run_demo() {
    let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
    client.ui_attach(80, 24).await.expect("attach");

    // Drain initial events
    let events = client.drain_events().await.unwrap();
    let mut grid = GridState::new(80, 24);
    apply_events(&mut grid, &events);

    // Type "Hello World"
    client.input("i").await.ok();
    client.input("Hello World").await.ok();
    client.input("\x1b").await.ok();

    // Force redraw and drain
    client.redraw_flush().await.ok();
    let events = client.drain_events().await.unwrap();
    apply_events(&mut grid, &events);

    print_grid(&grid, "After typing 'Hello World'");

    client.shutdown().await.ok();
}

/// Send an Ex command, print grid.
async fn run_exec(cmd: &str) {
    let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
    client.ui_attach(80, 24).await.expect("attach");

    let events = client.drain_events().await.unwrap();
    let mut grid = GridState::new(80, 24);
    apply_events(&mut grid, &events);

    client.command(cmd).await.expect("command");
    client.redraw_flush().await.ok();
    let events = client.drain_events().await.unwrap();
    apply_events(&mut grid, &events);

    print_grid(&grid, &format!("After :{cmd}"));
    client.shutdown().await.ok();
}

/// Send keystrokes, print grid.
async fn run_input(keys: &str) {
    let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
    client.ui_attach(80, 24).await.expect("attach");

    let events = client.drain_events().await.unwrap();
    let mut grid = GridState::new(80, 24);
    apply_events(&mut grid, &events);

    client.input(keys).await.expect("input");
    client.redraw_flush().await.ok();
    let events = client.drain_events().await.unwrap();
    apply_events(&mut grid, &events);

    print_grid(&grid, &format!("After input: {keys}"));
    client.shutdown().await.ok();
}

/// Attach, drain, print grid.
async fn run_dump() {
    let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
    client.ui_attach(80, 24).await.expect("attach");

    let events = client.drain_events().await.unwrap();
    let mut grid = GridState::new(80, 24);
    apply_events(&mut grid, &events);

    print_grid(&grid, "Initial grid after attach");
    client.shutdown().await.ok();
}

fn apply_events(grid: &mut GridState, events: &[wolpa_core::event::UiEvent]) {
    for event in events {
        if let wolpa_core::event::UiEvent::Redraw(ref redraw_events) = event {
            grid.apply_batch(redraw_events);
        }
    }
}

/// Print the grid state to stdout in a human-readable format.
fn print_grid(grid: &GridState, title: &str) {
    println!("\n=== {title} ===");
    for (&id, g) in &grid.grids {
        let content_rows = g.height;
        let show_rows = content_rows.min(24);

        println!(
            "── Grid {id} ({w}×{h}) ──",
            id = id,
            w = g.width,
            h = g.height
        );
        for r in 0..show_rows {
            let line: String = (0..g.width)
                .map(|c| {
                    let cell = g.cell(r, c);
                    if cell.text == " " && cell.hl_id == 0 {
                        '.'
                    } else {
                        cell.text.chars().next().unwrap_or('?')
                    }
                })
                .collect();
            println!("{:2} │{}│", r, line);
        }
    }
    let _ = std::io::stdout().flush();
}
