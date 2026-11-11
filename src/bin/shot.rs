//! Снимает экраны всех режимов просмотра в файлы PPM.
//!
//! Утилита переиспользует настоящий `draw_snapshot`, поэтому снимки совпадают
//! с тем, что видит пользователь в окне.
//!
//! Запуск: `cargo run --bin shot`
//! Результат: PPM-файлы в `target/shots/`. Для PNG: `convert target/shots/*.ppm *.png`

#[allow(dead_code)]
#[path = "../main.rs"]
mod app;

use std::path::{Path, PathBuf};

use app::{Jar, ViewMode, draw_frame, seeded_jar};

/// Каталог для снимков.
fn output_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/shots");
    std::fs::create_dir_all(&dir).expect("не удалось создать каталог для снимков");
    dir
}

/// Параметры одного снимка.
struct Shot<'a> {
    /// Куда сохранять.
    dir: &'a Path,
    /// Имя файла без расширения.
    name: &'a str,
    /// Состояние мира.
    snapshot: &'a app::JarSnapshot,
    /// Режим просмотра.
    view: ViewMode,
    /// Размер клетки в пикселях.
    cell: u32,
    /// Позиция курсора.
    cursor: (usize, usize),
    /// Ширина окна в пикселях.
    width: u32,
    /// Высота окна в пикселях.
    height: u32,
}

/// Один снимок: PPM с тем же раскладом пикселей, что и буфер softbuffer.
fn render(shot: Shot<'_>) {
    let Shot {
        dir,
        name,
        snapshot,
        view,
        cell,
        cursor,
        width,
        height,
    } = shot;

    let mut buffer = vec![0u32; (width * height) as usize];
    draw_frame(&mut buffer, width, height, snapshot, view, cell, cursor);

    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in &buffer {
        data.push((pixel >> 16) as u8);
        data.push((pixel >> 8) as u8);
        data.push(*pixel as u8);
    }

    let path = dir.join(format!("{name}.ppm"));
    std::fs::write(&path, &data).expect("не удалось записать снимок");
    println!("{}", path.display());
}

/// Состояние банки после `ticks` тиков.
fn state_after(ticks: usize) -> (Jar, app::JarSnapshot) {
    let mut jar = seeded_jar();
    for _ in 0..ticks {
        jar.simulation_step();
    }
    let snapshot = jar.snapshot();
    (jar, snapshot)
}

fn main() {
    let dir = output_dir();

    // Полный вид банки: видны все четыре колонии.
    let (_, snapshot) = state_after(200);
    render(Shot {
        dir: &dir,
        name: "creatures-z8",
        snapshot: &snapshot,
        view: ViewMode::Creatures,
        cell: 8,
        cursor: (50, 50),
        width: 900,
        height: 900,
    });

    // Приближение на верхнюю колонию.
    for (name, view) in [
        ("creatures-z32", ViewMode::Creatures),
        ("creatures-z64", ViewMode::Creatures),
        ("minerals-z32", ViewMode::Minerals),
        ("energy-z32", ViewMode::Energy),
        ("genome-z32", ViewMode::Genome),
        ("action-z32", ViewMode::Action),
    ] {
        render(Shot {
            dir: &dir,
            name,
            snapshot: &snapshot,
            view,
            cell: if name.ends_with("z64") { 64 } else { 32 },
            cursor: (50, 4),
            width: 900,
            height: 900,
        });
    }

    // Свет целиком виден только когда в кадр влезает вся банка.
    for (name, cell) in [("light-z8", 8), ("light-z16", 16)] {
        render(Shot {
            dir: &dir,
            name,
            snapshot: &snapshot,
            view: ViewMode::Light,
            cell,
            cursor: (50, 50),
            width: 900,
            height: 900,
        });
    }

    // Минимальный масштаб: сетка почти съедает поле.
    render(Shot {
        dir: &dir,
        name: "creatures-z4",
        snapshot: &snapshot,
        view: ViewMode::Creatures,
        cell: 4,
        cursor: (50, 50),
        width: 900,
        height: 900,
    });

    // То, что пользователь видит при запуске окна: поле вписано в окно целиком.
    let (_, start) = state_after(0);
    for (name, cell) in [("startup", 8), ("startup-zoom", 32)] {
        render(Shot {
            dir: &dir,
            name,
            snapshot: &start,
            view: ViewMode::Creatures,
            cell,
            cursor: (50, 2),
            width: 800,
            height: 800,
        });
    }
}
