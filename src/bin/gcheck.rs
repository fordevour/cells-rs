//! Печатает цвета и состав геномов: проверка, что режим Genome различает
//! колонии, а стартовые геномы отличаются друг от друга по родству.
//!
//! Запуск: `cargo run --bin gcheck`

#[allow(dead_code)]
#[path = "../main.rs"]
mod app;

use app::{
    ActionKind, CreatureAction, CreatureGenome, genome_bottom, genome_color, genome_left,
    genome_right, genome_top,
};

/// Печатает цвет генома в виде `#RRGGBB`.
fn describe(name: &str, genome: &CreatureGenome) {
    let color = genome_color(genome);
    let r = (color >> 16) & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = color & 0xFF;
    println!(
        "{name:>6}: #{r:02X}{g:02X}{b:02X}  (r={r:>3} g={g:>3} b={b:>3})  генов: {}",
        genome.len()
    );
}

/// Печатает состав ДНК: долю каждого типа генов в процентах.
fn describe_mix(name: &str, genome: &CreatureGenome) {
    let mix = genome.mix();
    let parts: Vec<String> = ActionKind::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| format!("{} {}%", kind.label(), mix[index] as u32 * 100 / 255))
        .collect();

    println!("{name:>6}: {}", parts.join(" "));
}

fn main() {
    println!("=== стартовые геномы ===");
    describe("top", &genome_top());
    describe("right", &genome_right());
    describe("bottom", &genome_bottom());
    describe("left", &genome_left());

    println!();
    println!("=== состав стартовых геномов ===");
    describe_mix("top", &genome_top());
    describe_mix("right", &genome_right());
    describe_mix("bottom", &genome_bottom());
    describe_mix("left", &genome_left());

    println!();
    println!("=== родство стартовых геномов (сходство в процентах) ===");
    let genomes = [
        ("top", genome_top()),
        ("right", genome_right()),
        ("bottom", genome_bottom()),
        ("left", genome_left()),
    ];

    for (index, (name, genome)) in genomes.iter().enumerate() {
        for (other_name, other) in genomes.iter().skip(index + 1) {
            println!(
                "{name:>6} и {other_name:>6}: {:>3}%",
                genome.similarity_to(other)
            );
        }
    }

    println!();
    println!("=== чистые геномы ===");
    for action in [
        CreatureAction::PhotoGrow,
        CreatureAction::EatMinerals,
        CreatureAction::Split,
        CreatureAction::Jump(1),
        CreatureAction::Share,
        CreatureAction::Attack,
    ] {
        let genome = CreatureGenome::from_line(vec![action; 16]);
        describe(&format!("{action:?}"), &genome);
    }

    println!();
    println!("=== чувствительность к мутации (стандартный геном + 1 замена) ===");
    let base = genome_top();
    let base_color = genome_color(&base);
    for index in 0..base.len() {
        let mutated = base.clone().with_genes(&[(index, CreatureAction::Jump(1))]);
        let color = genome_color(&mutated);
        let delta = color_diff(base_color, color);
        println!("замена гена {index:>2}: #{color:06X}  отличие от исходного: {delta}");
    }
}

/// Разница двух цветов как евклидово расстояние в RGB.
fn color_diff(a: u32, b: u32) -> u32 {
    let dr = ((a >> 16) & 0xFF).abs_diff((b >> 16) & 0xFF);
    let dg = ((a >> 8) & 0xFF).abs_diff((b >> 8) & 0xFF);
    let db = (a & 0xFF).abs_diff((b & 0xFF) & 0xFF);
    dr * dr + dg * dg + db * db
}
