//! Печатает цвета геномов: проверка, что режим Genome различает колонии.
//!
//! Запуск: `cargo run --bin gcheck`

#[allow(dead_code)]
#[path = "../main.rs"]
mod app;

use app::{
    CreatureAction, CreatureGenome, genome_bottom, genome_color, genome_left, genome_right,
    genome_top,
};

/// Печатает цвет генома в виде `#RRGGBB`.
fn describe(name: &str, genome: &CreatureGenome) {
    let color = genome_color(genome);
    let r = (color >> 16) & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = color & 0xFF;
    println!(
        "{name:>6}: #{r:02X}{g:02X}{b:02X}  (r={r:>3} g={g:>3} b={b:>3})  генов: {}",
        genome.line.len()
    );
}

fn main() {
    println!("=== стартовые геномы ===");
    describe("top", &genome_top());
    describe("right", &genome_right());
    describe("bottom", &genome_bottom());
    describe("left", &genome_left());

    println!();
    println!("=== чистые геномы ===");
    for action in [
        CreatureAction::PhotoGrow,
        CreatureAction::EatMinerals,
        CreatureAction::Split,
        CreatureAction::Jump(1),
    ] {
        let genome = CreatureGenome::from_line(vec![action; 16]);
        describe(&format!("{action:?}"), &genome);
    }

    println!();
    println!("=== чувствительность к мутации (стандартный геном + 1 замена) ===");
    let base = genome_top();
    let base_color = genome_color(&base);
    for index in 0..base.line.len() {
        let mut mutated = base.clone();
        mutated.line[index] = CreatureAction::Jump(1);
        let color = genome_color(&mutated);
        let delta = color_diff(base_color, color);
        println!("замена гена {index:>2}: #{color:06X}  отличие от исходного: {delta}");
    }
}

/// Разница двух цветов как евклидово расстояние в RGB.
fn color_diff(a: u32, b: u32) -> u32 {
    let dr = ((a >> 16) & 0xFF).abs_diff((b >> 16) & 0xFF);
    let dg = ((a >> 8) & 0xFF).abs_diff((b >> 8) & 0xFF);
    let db = (a & 0xFF).abs_diff(b & 0xFF);
    dr * dr + dg * dg + db * db
}
