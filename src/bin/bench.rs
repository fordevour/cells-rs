//! Замеряет время отрисовки по режимам просмотра и масштабам.
//!
//! Утилита нужна, чтобы проверять, что визуальные правки не бьют по
//! частоте кадров: бюджет — 16.7 мс на кадр при 60 fps.
//!
//! Запуск: `cargo run --release --bin bench`

#[allow(dead_code)]
#[path = "../main.rs"]
mod app;

use std::time::{Duration, Instant};

use app::{BACKGROUND_COLOR, ViewMode, draw_snapshot, seeded_jar};

/// Бюджет одного кадра при 60 fps.
const FRAME_BUDGET: Duration = Duration::from_micros(16_667);

/// Сколько кадров усредняем.
const ITERATIONS: u32 = 60;

fn bench(view: ViewMode, cell: u32, width: u32, height: u32) {
    let jar = seeded_jar();
    let snapshot = jar.snapshot();
    let mut buffer = vec![BACKGROUND_COLOR; (width * height) as usize];

    // Прогрев, чтобы не мерить компиляцию и первый вызов.
    for _ in 0..5 {
        draw_snapshot(&mut buffer, width, height, &snapshot, view, cell, 50, 50);
    }

    let start = Instant::now();
    for _ in 0..ITERATIONS {
        draw_snapshot(&mut buffer, width, height, &snapshot, view, cell, 50, 50);
    }
    let per_frame = start.elapsed() / ITERATIONS;

    println!(
        "{:>9} cell={:>2} -> {:>7.3} ms/frame  (бюджет {:.1} ms, запас {:.0}x)",
        view.label(),
        cell,
        per_frame.as_secs_f64() * 1000.0,
        FRAME_BUDGET.as_secs_f64() * 1000.0,
        FRAME_BUDGET.as_secs_f64() / per_frame.as_secs_f64(),
    );
}

fn main() {
    let (width, height) = (800, 800);

    println!("=== отрисовка ===");
    for cell in [3u32, 4, 8, 16, 32, 64] {
        bench(ViewMode::Creatures, cell, width, height);
    }
    println!();
    for cell in [3u32, 4, 8, 16, 32, 64] {
        bench(ViewMode::Light, cell, width, height);
    }
    println!();

    println!("=== снимок состояния ===");
    let jar = seeded_jar();
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = jar.snapshot();
    }
    let per_snapshot = start.elapsed() / ITERATIONS;
    println!(
        "snapshot() -> {:>7.3} ms",
        per_snapshot.as_secs_f64() * 1000.0
    );

    println!();
    println!("=== симуляция ===");
    let mut jar = seeded_jar();
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        jar.simulation_step();
    }
    let per_tick = start.elapsed() / ITERATIONS;
    println!(
        "simulation_step() -> {:>7.3} ms",
        per_tick.as_secs_f64() * 1000.0
    );
}
