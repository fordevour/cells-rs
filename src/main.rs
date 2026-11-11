use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const FRAME_TIME: Duration = Duration::from_micros(16_667);
const MIN_CELL_SIZE: u32 = 3;
const MAX_CELL_SIZE: u32 = 64;

/// Задержка перед автоповтором перемещения выделения.
const REPEAT_DELAY: Duration = Duration::from_millis(180);
/// Интервал автоповтора в начале удержания клавиши.
const REPEAT_INTERVAL_START: Duration = Duration::from_millis(45);
/// Минимальный интервал автоповтора: предельная скорость перемещения.
const REPEAT_INTERVAL_MIN: Duration = Duration::from_millis(12);
/// Сколько тиков живет клетка, после чего умирает.
const CREATURE_LIFETIME_TICKS: usize = 1000;
/// Максимальный прирост энергии от фотосинтеза за тик (у самого края).
const MAX_PHOTOSYNTHESIS_ENERGY: usize = 4;
/// Сколько энергии тратит поедание одного минерала.
const EAT_MINERALS_ENERGY_COST: usize = 1;
/// Шанс мутации генома при делении, в процентах (для каждой дочерней клетки).
const MUTATION_CHANCE_PERCENT: u64 = 5;
/// Максимальная скорость: столько тиков за кадр.
const MAX_SPEED: u32 = u32::MAX;
/// Максимальное замедление: один тик раз в столько кадров.
const MAX_FRAMES_PER_TICK: u32 = 64;

/// Насколько сильно вспышка рождения или смерти перекрывает клетку.
const FLASH_STRENGTH: u32 = 200;
/// Сколько кадров длится вспышка рождения или смерти.
const FLASH_FRAMES: u8 = 14;
/// Яркость клетки в самой темной точке банки.
const DARK_FACTOR: u32 = 40;
/// Насколько сильно освещенные края уходят в теплый оттенок солнца.
const WARM_STRENGTH: u32 = 90;
/// Насколько быстро цвет клетки догоняет новые данные в режимах-тепкартах.
const TRAIL_SPEED: u32 = 96;
/// Как часто обновлять заголовок окна.
const TITLE_INTERVAL: Duration = Duration::from_millis(200);
/// Высота рамки банки в пикселях.
const JAR_FRAME_THICKNESS: u32 = 3;
/// Высота внутренней тени банки в пикселях.
const JAR_SHADOW_THICKNESS: u32 = 2;
/// Размер стороны мини-карты в пикселях (одна клетка — один пиксель).
const MINIMAP_SIZE: u32 = 100;
/// Отступ панелей от края окна в пикселях.
const PANEL_MARGIN: u32 = 6;
/// Ширина полосы легенды в пикселях.
const LEGEND_WIDTH: u32 = 120;
/// Высота полосы легенды в пикселях.
const LEGEND_HEIGHT: u32 = 10;
/// Ширина глифа шрифта в пикселях.
const FONT_WIDTH: u32 = 8;
/// Высота глифа шрифта в пикселях.
const FONT_HEIGHT: u32 = 8;
/// Шаг символов шрифта в пикселях.
const TEXT_ADVANCE: u32 = FONT_WIDTH + 1;
/// Внутренний отступ панели в пикселях.
const PANEL_PADDING: u32 = 4;

/// Палитра отрисовки: все цвета в одном месте.
struct Palette {
    /// Фон окна вокруг банки.
    window_bg: u32,
    /// Фон внутри банки: пустая клетка.
    jar_bg: u32,
    /// Линии сетки.
    grid: u32,
    /// Рамка банки.
    jar_frame: u32,
    /// Тень под рамкой банки.
    jar_shadow: u32,
    /// Обычное существо.
    creature: u32,
    /// Тёмная часть курсора.
    cursor_dark: u32,
    /// Светлая часть курсора.
    cursor_light: u32,
    /// Фон панели HUD.
    panel_bg: u32,
    /// Рамка панели HUD.
    panel_border: u32,
    /// Текст HUD.
    panel_text: u32,
    /// Цвета типов действий: PhotoGrow, EatMinerals, Split, Jump.
    action_colors: [u32; 4],
}

impl Palette {
    /// Стандартная палитра.
    const fn new() -> Self {
        Self {
            window_bg: 0x00E9_EBEE,
            jar_bg: 0x00F7_F8FA,
            grid: 0x00E1_E5EA,
            jar_frame: 0x002B_2F36,
            jar_shadow: 0x00C8_CCD2,
            creature: 0x003F_B950,
            cursor_dark: 0x001F_2430,
            cursor_light: 0x00FF_FFFF,
            panel_bg: 0x00FF_FFFF,
            panel_border: 0x002B_2F36,
            panel_text: 0x001F_2430,
            action_colors: [
                0x003F_B950, // PhotoGrow
                0x002C_74C4, // EatMinerals
                0x00E8_590C, // Split
                0x00C2_255C, // Jump
            ],
        }
    }
}

/// Рампа минералов: холодная, от светлого к тёмному.
const MINERAL_RAMP: [(u32, (u32, u32, u32)); 5] = [
    (0, (207, 227, 245)),
    (64, (159, 201, 236)),
    (128, (95, 168, 222)),
    (192, (44, 116, 196)),
    (255, (18, 63, 126)),
];

/// Рампа энергии: тёплая, от светлого к тёмному.
const ENERGY_RAMP: [(u32, (u32, u32, u32)); 5] = [
    (0, (255, 224, 184)),
    (64, (255, 192, 120)),
    (128, (247, 148, 72)),
    (192, (222, 90, 30)),
    (255, (122, 46, 8)),
];

/// Простой генератор псевдослучайных чисел (xorshift64*).
pub struct Rng(u64);

impl Rng {
    /// Создает генератор с заданным зерном. Нулевое зерно заменяется на ненулевое.
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// Создает генератор, засеянный текущим временем.
    pub fn from_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::new(seed)
    }

    /// Следующее псевдослучайное число.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Случайное число в диапазоне `0..upper`.
    fn below(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        (self.next_u64() % upper as u64) as usize
    }

    /// Возвращает `true` с вероятностью `percent` процентов.
    fn chance_percent(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }
}

/// Случайный ген для вставки или замены.
fn random_action(rng: &mut Rng) -> CreatureAction {
    match rng.below(4) {
        0 => CreatureAction::PhotoGrow,
        1 => CreatureAction::EatMinerals,
        2 => CreatureAction::Split,
        _ => CreatureAction::Jump(random_jump_delta(rng)),
    }
}

/// Случайное ненулевое значение для гена Jump.
fn random_jump_delta(rng: &mut Rng) -> isize {
    let delta = rng.below(8) as isize - 4;
    if delta >= 0 { delta + 1 } else { delta }
}

fn main() {
    let jar = seeded_jar();

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(err) => {
            eprintln!("Не удалось создать event loop: {err}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(jar);
    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("Ошибка event loop: {err}");
    }
}

/// Создает Jar с четырьмя группами существ у разных краев.
pub(crate) fn seeded_jar() -> Jar {
    let mut jar = Jar::new(100);
    let size = jar.size;
    let mid = size / 2;

    // По три клетки на каждом краю, рядом с зоной максимального света.
    spawn(
        &mut jar,
        genome_top(),
        &[(mid - 1, 2), (mid, 2), (mid + 1, 2)],
    );
    spawn(
        &mut jar,
        genome_right(),
        &[(size - 3, mid - 1), (size - 3, mid), (size - 3, mid + 1)],
    );
    spawn(
        &mut jar,
        genome_bottom(),
        &[(mid - 1, size - 3), (mid, size - 3), (mid + 1, size - 3)],
    );
    spawn(
        &mut jar,
        genome_left(),
        &[(2, mid - 1), (2, mid), (2, mid + 1)],
    );

    jar
}

/// Стандартный геном: верхний край.
pub(crate) fn genome_top() -> CreatureGenome {
    CreatureGenome::standard()
}

/// Прыжки по ДНК вместо части делений: правый край.
pub(crate) fn genome_right() -> CreatureGenome {
    let mut genome = CreatureGenome::standard();
    genome.line[3] = CreatureAction::Jump(1);
    genome.line[7] = CreatureAction::Jump(2);
    genome.line[11] = CreatureAction::Jump(1);
    genome
}

/// Больше поедания минералов: нижний край.
pub(crate) fn genome_bottom() -> CreatureGenome {
    let mut genome = CreatureGenome::standard();
    genome.line[4] = CreatureAction::EatMinerals;
    genome.line[8] = CreatureAction::EatMinerals;
    genome
}

/// Больше делений: левый край.
pub(crate) fn genome_left() -> CreatureGenome {
    let mut genome = CreatureGenome::standard();
    genome.line[2] = CreatureAction::Split;
    genome.line[6] = CreatureAction::Split;
    genome
}

/// Размещает одинаковые геномы в указанных клетках поля.
fn spawn(jar: &mut Jar, genome: CreatureGenome, positions: &[(usize, usize)]) {
    let size = jar.size;
    for &(x, y) in positions {
        jar.creatures[x + y * size] = Some(Creature {
            genome: genome.clone(),
            energy: 0,
            minerals: 0,
            age: 0,
            last_action: None,
        });
    }
}

/// Скорость симуляции: либо несколько тиков за кадр, либо один тик раз в несколько кадров.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Speed {
    /// Столько тиков выполнять за один кадр.
    TicksPerFrame(u32),
    /// Один тик выполнять раз в столько кадров.
    FramesPerTick(u32),
}

impl Speed {
    /// Следующая, более высокая скорость.
    fn faster(self) -> Self {
        match self {
            Self::TicksPerFrame(n) => Self::TicksPerFrame(if n == MAX_SPEED { n } else { n + 1 }),
            Self::FramesPerTick(2) => Self::TicksPerFrame(1),
            Self::FramesPerTick(n) => Self::FramesPerTick(n - 1),
        }
    }

    /// Следующая, более низкая скорость.
    fn slower(self) -> Self {
        match self {
            Self::TicksPerFrame(1) => Self::FramesPerTick(2),
            Self::TicksPerFrame(n) => Self::TicksPerFrame(n - 1),
            Self::FramesPerTick(n) => {
                Self::FramesPerTick(n.saturating_add(1).min(MAX_FRAMES_PER_TICK))
            }
        }
    }

    /// Подпись скорости для заголовка окна.
    fn label(self) -> String {
        match self {
            Self::TicksPerFrame(n) => format!("x{n}"),
            Self::FramesPerTick(n) => format!("x1/{n}"),
        }
    }
}

/// Режим просмотра поля.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewMode {
    /// Основной режим: только существа.
    Creatures,
    /// Только концентрация минералов.
    Minerals,
    /// Только энергия существ.
    Energy,
    /// Освещенность: свет у краев Jar и тьма в центре.
    Light,
    /// Цвета геномов: похожие ДНК окрашены похоже.
    Genome,
    /// Текущее действие каждой клетки: фотосинтез, еда, деление, прыжок.
    Action,
}

impl ViewMode {
    /// Следующий режим по кругу.
    fn next(self) -> Self {
        match self {
            Self::Creatures => Self::Minerals,
            Self::Minerals => Self::Energy,
            Self::Energy => Self::Light,
            Self::Light => Self::Genome,
            Self::Genome => Self::Action,
            Self::Action => Self::Creatures,
        }
    }

    /// Подпись режима для заголовка окна.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Creatures => "creatures",
            Self::Minerals => "minerals",
            Self::Energy => "energy",
            Self::Light => "light",
            Self::Genome => "genome",
            Self::Action => "action",
        }
    }

    /// Нужна ли сетка в этом режиме: без неё не видно границы клеток.
    fn needs_grid(self) -> bool {
        matches!(
            self,
            Self::Minerals | Self::Energy | Self::Genome | Self::Action
        )
    }

    /// Рисуется ли в этом режиме контур колоний вместо сетки.
    fn needs_outline(self) -> bool {
        matches!(self, Self::Creatures | Self::Light)
    }

    /// Сглаживается ли в этом режиме цвет клетки между кадрами.
    fn needs_trails(self) -> bool {
        matches!(self, Self::Minerals | Self::Energy)
    }
}

/// Направление перемещения выделения.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// Сдвиг на одну клетку.
    fn delta(self) -> (isize, isize) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }

    /// Направление по нажатой клавише.
    fn from_key(key: PhysicalKey) -> Option<Self> {
        match key {
            PhysicalKey::Code(KeyCode::KeyW | KeyCode::ArrowUp) => Some(Self::Up),
            PhysicalKey::Code(KeyCode::KeyS | KeyCode::ArrowDown) => Some(Self::Down),
            PhysicalKey::Code(KeyCode::KeyA | KeyCode::ArrowLeft) => Some(Self::Left),
            PhysicalKey::Code(KeyCode::KeyD | KeyCode::ArrowRight) => Some(Self::Right),
            _ => None,
        }
    }
}

/// Интервал автоповтора: чем дольше удерживаем клавишу, тем быстрее едем.
///
/// За первую секунду интервал падает с 45 мс до 12 мс: это примерно
/// 22 клетки в секунду в начале и 83 клетки в секунду на пределе.
fn repeat_interval(held_for: Duration) -> Duration {
    let start = REPEAT_INTERVAL_START.as_millis() as u32;
    let minimum = REPEAT_INTERVAL_MIN.as_millis() as u32;
    let elapsed = held_for.as_millis() as u32;

    let interval = start
        .saturating_sub(elapsed * (start - minimum) / 1000)
        .max(minimum);

    Duration::from_millis(interval as u64)
}

/// Помещается ли поле целиком в окно при таком размере клетки.
fn field_fits(cell_size: u32, size: usize, width: u32, height: u32) -> bool {
    let grid = size as u32 * cell_size;
    grid <= width && grid <= height
}

/// Данные выбранной клетки для панели инспектора.
struct CellInfo {
    /// Координаты клетки в поле.
    x: usize,
    y: usize,
    /// Есть ли в клетке существо.
    alive: bool,
    /// Энергия существа.
    energy: usize,
    /// Минералы в самой клетке.
    minerals: usize,
    /// Сколько минералов съело существо.
    eaten: usize,
    /// Возраст существа в тиках.
    age: usize,
    /// Доли генов каждого типа в геноме.
    mix: Option<[u8; 4]>,
    /// Последнее выполненное действие.
    action: Option<CreatureAction>,
}

/// Приложение winit: хранит Jar, раз в кадр обновляет снапшот и рисует его.
struct App {
    jar: Jar,
    snapshot: JarSnapshot,

    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,

    /// Палитра отрисовки.
    palette: Palette,
    /// Размер одной клетки в пикселях.
    cell_size: u32,
    /// Размер клетки, при котором поле целиком влезает в окно.
    fit_cell_size: u32,
    /// Размер окна в пикселях: нужен, чтобы знать, помещается ли поле.
    window_size: (u32, u32),
    /// Позиция выбранной клетки.
    cursor_x: usize,
    cursor_y: usize,
    /// Направление, которое сейчас удерживается, и момент нажатия.
    hold: Option<(Direction, Instant)>,
    /// Когда сработает следующий автоповтор перемещения.
    next_repeat: Instant,
    /// Скорость симуляции.
    speed: Speed,
    /// Сколько кадров накоплено при замедленном режиме.
    frame_accum: u32,
    /// Приостановлена ли симуляция.
    paused: bool,
    /// Режим просмотра поля.
    view_mode: ViewMode,
    /// Вспышки рождения и смерти: сколько кадров осталось каждой клетке.
    flashes: Vec<u8>,
    /// Цвета клеток прошлого кадра: для сглаживания шкалы в тепкартах.
    cell_layer: Vec<u32>,
    /// Маска существ в прошлом кадре: по ней считаются вспышки.
    prev_creatures: Vec<bool>,
    /// Когда последний раз обновлялся заголовок окна.
    last_title: Instant,
    /// Заголовок окна, который уже выведен: обновляем только при изменении.
    title: String,
    next_frame: Instant,
}

impl App {
    fn new(jar: Jar) -> Self {
        let snapshot = jar.snapshot();
        let size = jar.size;
        let palette = Palette::new();
        // Стартовая камера смотрит на верхнюю колонию: в центре банки никого нет.
        let cursor_x = size / 2;
        let cursor_y = 2;
        Self {
            jar,
            flashes: vec![0; size * size],
            cell_layer: vec![palette.jar_bg; size * size],
            prev_creatures: snapshot.creatures.clone(),
            snapshot,
            window: None,
            context: None,
            surface: None,
            palette,
            cell_size: 8,
            fit_cell_size: 8,
            window_size: (800, 800),
            cursor_x,
            cursor_y,
            hold: None,
            next_repeat: Instant::now(),
            speed: Speed::TicksPerFrame(1),
            frame_accum: 0,
            paused: false,
            view_mode: ViewMode::Creatures,
            last_title: Instant::now() - TITLE_INTERVAL,
            title: String::new(),
            next_frame: Instant::now(),
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Текст заголовка окна.
    fn title_text(&self) -> String {
        let pause = if self.paused { " | paused" } else { "" };
        format!(
            "Cells | tick {} | pop {} | zoom {} | speed {} | mode {}{}",
            self.snapshot.tick,
            self.snapshot.population,
            self.zoom_label(),
            self.speed.label(),
            self.view_mode.label(),
            pause
        )
    }

    /// Масштаб относительно вписанного вида.
    fn zoom_label(&self) -> String {
        if !self.selection_active() {
            return "fit".to_string();
        }

        let fit = self.fit_cell_size.max(1) as f32;
        format!("x{:.1}", self.cell_size as f32 / fit)
    }

    /// Обновляет заголовок окна, но не чаще чем раз в `TITLE_INTERVAL`.
    fn update_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };

        let now = Instant::now();
        if now.duration_since(self.last_title) < TITLE_INTERVAL {
            return;
        }

        let text = self.title_text();
        if text == self.title {
            return;
        }

        self.last_title = now;
        self.title = text.clone();
        window.set_title(&text);
    }

    /// Меняет скорость симуляции: delta > 0 — быстрее, delta < 0 — медленнее.
    fn change_speed(&mut self, delta: i32) {
        let new_speed = if delta > 0 {
            self.speed.faster()
        } else if delta < 0 {
            self.speed.slower()
        } else {
            self.speed
        };

        if new_speed != self.speed {
            self.speed = new_speed;
            self.frame_accum = 0;
            self.update_title();
        }
    }

    /// Ставит симуляцию на паузу или снимает ее.
    fn toggle_pause(&mut self) {
        self.paused = !self.paused;

        if !self.paused {
            // Возобновляем симуляцию сразу, не дожидаясь следующего кадра.
            self.next_frame = Instant::now();
        }

        self.update_title();
    }

    /// Переключает режим просмотра поля.
    fn toggle_view_mode(&mut self) {
        self.view_mode = self.view_mode.next();
        self.update_title();
        self.request_redraw();
    }

    fn zoom(&mut self, direction: i32) {
        let step = (self.cell_size / 8).max(1) as i32;
        let new_size = (self.cell_size as i32 + direction * step)
            .clamp(MIN_CELL_SIZE as i32, MAX_CELL_SIZE as i32) as u32;

        self.set_cell_size(new_size);
    }

    /// Вписывает поле в окно целиком.
    fn fit_zoom(&mut self) {
        self.set_cell_size(self.fit_cell_size);
    }

    /// Меняет размер клетки и пересчитывает выделение, если поле перестало
    /// помещаться в окно или снова поместилось.
    fn set_cell_size(&mut self, new_size: u32) {
        let new_size = new_size.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE);
        if new_size == self.cell_size {
            return;
        }

        let (width, height) = self.window_size;
        let was_fitting = field_fits(self.cell_size, self.snapshot.size, width, height);
        let will_fit = field_fits(new_size, self.snapshot.size, width, height);

        if was_fitting && !will_fit {
            // Поле перестало помещаться: выделение появляется в центре экрана,
            // чтобы камера осталась на месте.
            let (x, y) = self.center_cell();
            self.cursor_x = x;
            self.cursor_y = y;
        }

        self.cell_size = new_size;
        self.request_redraw();
    }

    /// Существует ли сейчас выделение клетки.
    ///
    /// Пока поле целиком помещается в окне, выделения нет: камера центрирует
    /// поле, курсор не нужен.
    fn selection_active(&self) -> bool {
        !field_fits(
            self.cell_size,
            self.snapshot.size,
            self.window_size.0,
            self.window_size.1,
        )
    }

    /// Клетка, оказавшаяся в центре текущего вида.
    fn center_cell(&self) -> (usize, usize) {
        let (offset_x, offset_y) = field_offset(
            (self.cursor_x, self.cursor_y),
            self.snapshot.size,
            self.cell_size,
            self.window_size.0,
            self.window_size.1,
        );

        let cell = self.cell_size.max(1) as i64;
        let last = self.snapshot.size.saturating_sub(1) as i64;

        let x = ((self.window_size.0 as i64 / 2 - offset_x) / cell).clamp(0, last) as usize;
        let y = ((self.window_size.1 as i64 / 2 - offset_y) / cell).clamp(0, last) as usize;

        (x, y)
    }

    /// Сдвигает выделение на одну клетку в заданном направлении.
    fn move_selection(&mut self, direction: Direction) {
        // Пока поле помещается в окно, выделения нет: двигать нечего.
        if !self.selection_active() {
            return;
        }

        let (dx, dy) = direction.delta();
        let last = self.snapshot.size.saturating_sub(1) as isize;

        self.cursor_x = (self.cursor_x as isize + dx).clamp(0, last) as usize;
        self.cursor_y = (self.cursor_y as isize + dy).clamp(0, last) as usize;
        self.request_redraw();
    }

    /// Нажата клавиша движения: первый сдвиг и запуск автоповтора.
    fn press_direction(&mut self, direction: Direction) {
        self.hold = Some((direction, Instant::now()));
        self.next_repeat = Instant::now() + REPEAT_DELAY;
        self.move_selection(direction);
    }

    /// Клавиша движения отпущена: автоповтор прекращается.
    fn release_direction(&mut self) {
        self.hold = None;
    }

    /// Автоповтор: пока клавиша удерживается, выделение едет.
    fn update_repeat(&mut self, now: Instant) {
        let Some((direction, since)) = self.hold else {
            return;
        };

        if now < self.next_repeat {
            return;
        }

        self.move_selection(direction);
        self.next_repeat = now + repeat_interval(now.duration_since(since));
    }

    /// Пересчитывает вспышки рождения и смерти по разнице двух кадров.
    fn update_flashes(&mut self) {
        for (index, &alive) in self.snapshot.creatures.iter().enumerate() {
            let was_alive = self.prev_creatures[index];
            if alive != was_alive {
                self.flashes[index] = FLASH_FRAMES;
            } else if self.flashes[index] > 0 {
                self.flashes[index] -= 1;
            }
        }

        self.prev_creatures
            .copy_from_slice(&self.snapshot.creatures);
    }

    /// Данные выбранной клетки для инспектора.
    fn cell_info(&self) -> CellInfo {
        let size = self.jar.size;
        let index = self.cursor_x + self.cursor_y * size;

        match &self.jar.creatures[index] {
            Some(creature) => CellInfo {
                x: self.cursor_x,
                y: self.cursor_y,
                alive: true,
                energy: creature.energy,
                minerals: self.jar.minerals[index],
                eaten: creature.minerals,
                age: creature.age,
                mix: Some(genome_mix(&creature.genome)),
                action: creature.last_action,
            },
            None => CellInfo {
                x: self.cursor_x,
                y: self.cursor_y,
                alive: false,
                energy: 0,
                minerals: self.jar.minerals[index],
                eaten: 0,
                age: 0,
                mix: None,
                action: None,
            },
        }
    }

    fn render(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };

        // Всё, что читает состояние мира, считаем до того, как взяли поверхность.
        let selection_active = self.selection_active();
        let info = self.cell_info();
        let minerals: usize = self.snapshot.minerals.iter().sum();
        let hud_top = format!(
            "tick {}  pop {}  min {}",
            self.snapshot.tick,
            self.snapshot.population,
            format_count(minerals)
        );
        let hud_bottom = format!(
            "mode {}  speed {}  zoom {}{}",
            self.view_mode.label(),
            self.speed.label(),
            self.zoom_label(),
            if self.paused { "  PAUSED" } else { "" }
        );

        {
            let Some(surface) = self.surface.as_mut() else {
                return;
            };

            if let Err(err) = surface.resize(width, height) {
                eprintln!("Не удалось изменить размер поверхности: {err}");
                return;
            }

            let Ok(mut buffer) = surface.buffer_mut() else {
                return;
            };

            let mut context = RenderContext {
                snapshot: &self.snapshot,
                view: self.view_mode,
                palette: &self.palette,
                cell_size: self.cell_size,
                flashes: &self.flashes,
                cell_layer: &mut self.cell_layer,
                selection_active,
                cursor: (self.cursor_x, self.cursor_y),
                info,
                hud_top,
                hud_bottom,
            };

            let mut canvas = Canvas {
                buffer: &mut buffer,
                width: width.get(),
                height: height.get(),
            };
            draw_snapshot(&mut canvas, &mut context);

            if let Err(err) = buffer.present() {
                eprintln!("Не удалось показать кадр: {err}");
            }
        }

        self.update_title();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Cells")
            .with_inner_size(LogicalSize::new(800.0, 800.0))
            .with_min_inner_size(LogicalSize::new(320.0, 320.0));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("Не удалось создать окно: {err}");
                event_loop.exit();
                return;
            }
        };

        let context = match Context::new(window.clone()) {
            Ok(context) => context,
            Err(err) => {
                eprintln!("Не удалось создать softbuffer context: {err}");
                event_loop.exit();
                return;
            }
        };

        let surface = match Surface::new(&context, window.clone()) {
            Ok(surface) => surface,
            Err(err) => {
                eprintln!("Не удалось создать softbuffer surface: {err}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);

        // Стартовый вид: вся банка целиком в окне.
        let size = self
            .window
            .as_ref()
            .expect("окно только что создано")
            .inner_size();
        self.window_size = (size.width, size.height);
        self.fit_cell_size = fit_cell_size(size.width, size.height, self.jar.size);
        self.cell_size = self.fit_cell_size;

        self.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.surface = None;
        self.context = None;
        self.window = None;
        // Клавиша отпускания может не прийти: останавливаем автоповтор.
        self.release_direction();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.window_size = (size.width, size.height);
                self.fit_cell_size = fit_cell_size(size.width, size.height, self.jar.size);
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Focused(false) => {
                // При потере фокуса окно не пришлёт отпускание клавиши.
                self.release_direction();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Клавиши перемещения работают в обоих состояниях:
                // нажатие двигает выделение, отпускание останавливает автоповтор.
                if let Some(direction) = Direction::from_key(event.physical_key) {
                    match event.state {
                        ElementState::Pressed => {
                            // Системный автоповтор игнорируем: свой считаем сами,
                            // иначе выделение сдвигалось бы на две клетки.
                            if !event.repeat {
                                self.press_direction(direction);
                            }
                        }
                        ElementState::Released => self.release_direction(),
                    }
                    return;
                }

                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Space) => self.toggle_pause(),
                        PhysicalKey::Code(KeyCode::Tab) => self.toggle_view_mode(),
                        PhysicalKey::Code(KeyCode::KeyF) => self.fit_zoom(),
                        PhysicalKey::Code(KeyCode::Equal | KeyCode::NumpadAdd) => {
                            self.change_speed(1)
                        }
                        PhysicalKey::Code(KeyCode::Minus | KeyCode::NumpadSubtract) => {
                            self.change_speed(-1)
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 50.0,
                };

                if scroll > 0.0 {
                    self.zoom(1);
                } else if scroll < 0.0 {
                    self.zoom(-1);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        // Автоповтор перемещения работает и на паузе.
        self.update_repeat(now);

        if self.paused {
            // На паузе спим, но если клавиша удерживается, просыпаемся ради неё.
            if self.hold.is_some() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_repeat));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            return;
        }

        if now >= self.next_frame {
            let stepped = match self.speed {
                Speed::TicksPerFrame(n) => {
                    for _ in 0..n {
                        self.jar.simulation_step();
                    }
                    true
                }
                Speed::FramesPerTick(n) => {
                    self.frame_accum += 1;
                    if self.frame_accum >= n {
                        self.frame_accum = 0;
                        self.jar.simulation_step();
                        true
                    } else {
                        false
                    }
                }
            };

            if stepped {
                self.snapshot = self.jar.snapshot();
                self.update_flashes();
                self.request_redraw();
            }
            self.next_frame = now + FRAME_TIME;
        }

        // Просыпаемся на кадр симуляции или на момент автоповтора — что раньше.
        let mut wake = self.next_frame;
        if self.hold.is_some() && self.next_repeat < wake {
            wake = self.next_repeat;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

/// Всё, что нужно для отрисовки одного кадра.
struct RenderContext<'a> {
    /// Снимок состояния мира.
    snapshot: &'a JarSnapshot,
    /// Режим просмотра поля.
    view: ViewMode,
    /// Палитра отрисовки.
    palette: &'a Palette,
    /// Размер клетки в пикселях.
    cell_size: u32,
    /// Вспышки рождения и смерти по клеткам.
    flashes: &'a [u8],
    /// Цвета клеток прошлого кадра: нужны для сглаживания шкалы.
    cell_layer: &'a mut Vec<u32>,
    /// Есть ли сейчас выделение клетки.
    selection_active: bool,
    /// Выбранная клетка.
    cursor: (usize, usize),
    /// Данные выбранной клетки для инспектора.
    info: CellInfo,
    /// Строки панели HUD.
    hud_top: String,
    /// Вторая строка панели HUD.
    hud_bottom: String,
}

/// Смещение поля в пикселях.
///
/// Если поле влезает в окно целиком, оно центрируется, иначе камера
/// следует за курсором, который всегда остается в середине экрана.
fn field_offset(
    cursor: (usize, usize),
    size: usize,
    cell_size: u32,
    width: u32,
    height: u32,
) -> (i64, i64) {
    let cell = cell_size.max(1) as i64;
    let grid = size as i64 * cell;

    let offset_x = if grid <= width as i64 {
        (width as i64 - grid) / 2
    } else {
        width as i64 / 2 - (cursor.0 as i64 * cell + cell / 2)
    };
    let offset_y = if grid <= height as i64 {
        (height as i64 - grid) / 2
    } else {
        height as i64 / 2 - (cursor.1 as i64 * cell + cell / 2)
    };

    (offset_x, offset_y)
}

/// Размер клетки, при котором поле целиком влезает в окно.
fn fit_cell_size(width: u32, height: u32, size: usize) -> u32 {
    let side = width.min(height) / size.max(1) as u32;
    side.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE)
}

/// Рисует кадр в буфер: обёртка для инструментов и тестов.
///
/// Собирает контекст с нейтральными значениями: без указателя мыши,
/// без вспышек и с пустым инспектором.
#[allow(dead_code)]
pub(crate) fn draw_frame(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    snapshot: &JarSnapshot,
    view: ViewMode,
    cell_size: u32,
    cursor: (usize, usize),
) {
    let palette = Palette::new();
    let flashes = vec![0u8; snapshot.creatures.len()];
    let mut cell_layer = vec![palette.jar_bg; snapshot.creatures.len()];
    let index = cursor.0 + cursor.1 * snapshot.size;
    let info = CellInfo {
        x: cursor.0,
        y: cursor.1,
        alive: snapshot.creatures.get(index).copied().unwrap_or(false),
        energy: snapshot.energies.get(index).copied().unwrap_or(0),
        minerals: snapshot.minerals.get(index).copied().unwrap_or(0),
        eaten: 0,
        age: 0,
        mix: snapshot.genome_mix.get(index).copied().flatten(),
        action: snapshot.actions.get(index).copied().flatten(),
    };

    let mut context = RenderContext {
        snapshot,
        view,
        palette: &palette,
        cell_size,
        flashes: &flashes,
        cell_layer: &mut cell_layer,
        selection_active: !field_fits(cell_size, snapshot.size, width, height),
        cursor,
        info,
        hud_top: format!("tick {}  pop {}", snapshot.tick, snapshot.population),
        hud_bottom: format!("mode {}", view.label()),
    };

    let mut canvas = Canvas {
        buffer,
        width,
        height,
    };
    draw_snapshot(&mut canvas, &mut context);
}

/// Поверхность отрисовки: буфер пикселей и его размеры.
struct Canvas<'a> {
    /// Пиксели окна в том же порядке, что и у softbuffer.
    buffer: &'a mut [u32],
    /// Ширина окна в пикселях.
    width: u32,
    /// Высота окна в пикселях.
    height: u32,
}

impl Canvas<'_> {
    /// Заливает прямоугольник цветом. Части за пределами окна отсекаются.
    fn fill_rect(&mut self, x: i64, y: i64, rect_width: u32, rect_height: u32, color: u32) {
        let x0 = x.max(0).min(self.width as i64) as u32;
        let y0 = y.max(0).min(self.height as i64) as u32;
        let x1 = (x + rect_width as i64).max(0).min(self.width as i64) as u32;
        let y1 = (y + rect_height as i64).max(0).min(self.height as i64) as u32;

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        for row in y0..y1 {
            let start = (row * self.width + x0) as usize;
            let end = (row * self.width + x1) as usize;
            self.buffer[start..end].fill(color);
        }
    }

    /// Рисует один пиксель.
    fn pixel(&mut self, x: i64, y: i64, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }

        self.buffer[(y as u32 * self.width + x as u32) as usize] = color;
    }

    /// Заливает весь буфер цветом.
    fn fill(&mut self, color: u32) {
        self.buffer.fill(color);
    }
}

/// Рисует кадр: слои идут снизу вверх, порядок задан в плане.
fn draw_snapshot(canvas: &mut Canvas, context: &mut RenderContext) {
    let palette = context.palette;
    let snapshot = context.snapshot;
    let cell = context.cell_size.max(1);
    let grid = snapshot.size as u32 * cell;
    let (offset_x, offset_y) = field_offset(
        context.cursor,
        snapshot.size,
        cell,
        canvas.width,
        canvas.height,
    );

    // 0. фон окна вокруг банки
    canvas.fill(palette.window_bg);

    // 1. подложка банки
    canvas.fill_rect(offset_x, offset_y, grid, grid, palette.jar_bg);

    // 2. содержимое клеток
    draw_cells(canvas, context, offset_x, offset_y, cell);

    // 3. сетка: только там, где без неё не видно границы клеток
    if context.view.needs_grid() {
        draw_grid(
            canvas,
            snapshot.size,
            offset_x,
            offset_y,
            cell,
            palette.grid,
        );
    }

    // 4. контур колоний вместо сетки
    if context.view.needs_outline() {
        draw_colony_outline(
            canvas,
            snapshot,
            offset_x,
            offset_y,
            cell,
            palette.jar_frame,
        );
    }

    // 5. оверлей освещенности
    if context.view == ViewMode::Light {
        draw_light_overlay(canvas, offset_x, offset_y, grid, cell, snapshot.size);
    }
    // 6. рамка банки и внутренняя тень
    draw_jar_frame(canvas, offset_x, offset_y, grid, palette);

    // 7. курсор: пока поле помещается в окно, выделения нет
    if context.selection_active {
        draw_cursor(canvas, context, offset_x, offset_y, cell);
    }

    // 8. оверлеи интерфейса
    draw_minimap(canvas, context, offset_x, offset_y, cell);
    draw_hud(canvas, context);
    draw_legend(canvas, context);
    if context.selection_active {
        draw_inspector(canvas, context);
    }
}

/// Рисует содержимое клеток поля.
fn draw_cells(
    canvas: &mut Canvas,
    context: &mut RenderContext,
    offset_x: i64,
    offset_y: i64,
    cell: u32,
) {
    let snapshot = context.snapshot;
    let view = context.view;
    let palette = context.palette;
    let flashes = context.flashes;
    let layer = &mut *context.cell_layer;

    for y in 0..snapshot.size {
        for x in 0..snapshot.size {
            let index = x + y * snapshot.size;
            let mut color = cell_color(snapshot, index, view, palette);

            // В режимах-тепкартах цвет плавно догоняет новые данные:
            // скачок минералов или энергии не выглядит как вспышка.
            if view.needs_trails() {
                color = mix(layer[index], color, TRAIL_SPEED);
            }
            layer[index] = color;

            // Вспышка рождения или смерти.
            if let Some(flash) = flashes.get(index)
                && *flash > 0
            {
                let strength = (*flash as u32) * FLASH_STRENGTH / FLASH_FRAMES as u32;
                color = mix(color, 0x00FF_FFFF, strength);
            }

            let px = offset_x + (x as u32 * cell) as i64;
            let py = offset_y + (y as u32 * cell) as i64;
            canvas.fill_rect(px, py, cell, cell, color);

            // В режиме геномов внизу клетки показываем состав ДНК:
            // так видно мутацию, которую не показывает цвет.
            if view == ViewMode::Genome {
                draw_genome_bar(canvas, snapshot, index, px, py, cell, palette);
            }
        }
    }
}

/// Полоска состава генома внизу клетки: доли типов генов.
fn draw_genome_bar(
    canvas: &mut Canvas,
    snapshot: &JarSnapshot,
    index: usize,
    px: i64,
    py: i64,
    cell: u32,
    palette: &Palette,
) {
    // На мелком масштабе полоска нечитаема.
    if cell < 12 {
        return;
    }

    let Some(mix) = snapshot.genome_mix[index] else {
        return;
    };

    let bar_height = (cell / 6).clamp(3, 6);
    let y = py + cell as i64 - bar_height as i64;
    let mut x = px;

    for (part, share) in mix.iter().enumerate() {
        let part_width = (cell * *share as u32) / 255;
        canvas.fill_rect(x, y, part_width, bar_height, palette.action_colors[part]);
        x += part_width as i64;
    }

    // Добиваем остаток клетки последним цветом: полоска всегда во всю ширину.
    let rest = (px + cell as i64 - x).max(0) as u32;
    canvas.fill_rect(x, y, rest, bar_height, palette.action_colors[3]);
}

/// Цвет клетки в текущем режиме просмотра.
fn cell_color(snapshot: &JarSnapshot, index: usize, view: ViewMode, palette: &Palette) -> u32 {
    match view {
        ViewMode::Minerals => {
            let max = snapshot.max_minerals.max(1) as u32;
            let amount = snapshot.minerals[index].min(max as usize) as u32;
            ramp(amount * 255 / max, &MINERAL_RAMP)
        }
        ViewMode::Energy => {
            if !snapshot.creatures[index] {
                return palette.jar_bg;
            }
            let max = snapshot.max_energy.max(1) as u32;
            let amount = snapshot.energies[index].min(max as usize) as u32;
            ramp(amount * 255 / max, &ENERGY_RAMP)
        }
        ViewMode::Genome => snapshot.genome_colors[index].unwrap_or(palette.jar_bg),
        ViewMode::Action => match snapshot.actions[index] {
            Some(action) => palette.action_colors[action_index(action)],
            None => palette.jar_bg,
        },
        ViewMode::Creatures | ViewMode::Light => {
            if !snapshot.creatures[index] {
                return palette.jar_bg;
            }
            // Чем больше энергии, тем светлее клетка.
            let energy = snapshot.energies[index].min(100) as u32;
            mix(palette.creature, 0x00FF_FFFF, energy * 80 / 100)
        }
    }
}

/// Индекс типа действия в палитре.
fn action_index(action: CreatureAction) -> usize {
    match action {
        CreatureAction::PhotoGrow => 0,
        CreatureAction::EatMinerals => 1,
        CreatureAction::Split => 2,
        CreatureAction::Jump(_) => 3,
    }
}

/// Тонкая сетка по границам клеток.
fn draw_grid(
    canvas: &mut Canvas,
    size: usize,
    offset_x: i64,
    offset_y: i64,
    cell: u32,
    color: u32,
) {
    // На мелком масштабе сетка съедает поле, поэтому её отключаем.
    if cell < 6 {
        return;
    }

    let grid = size as u32 * cell;
    for i in 0..=size {
        let x = offset_x + (i as u32 * cell) as i64;
        let line_x = if i == size { x - 1 } else { x };
        canvas.fill_rect(line_x, offset_y, 1, grid, color);

        let y = offset_y + (i as u32 * cell) as i64;
        let line_y = if i == size { y - 1 } else { y };
        canvas.fill_rect(offset_x, line_y, grid, 1, color);
    }
}

/// Контур колонии: граница клетки рисуется только там, где рядом нет существа.
fn draw_colony_outline(
    canvas: &mut Canvas,
    snapshot: &JarSnapshot,
    offset_x: i64,
    offset_y: i64,
    cell: u32,
    color: u32,
) {
    let size = snapshot.size;

    for y in 0..size {
        for x in 0..size {
            if !snapshot.creatures[x + y * size] {
                continue;
            }

            let open_above = y == 0 || !snapshot.creatures[x + (y - 1) * size];
            let open_below = y + 1 == size || !snapshot.creatures[x + (y + 1) * size];
            let open_left = x == 0 || !snapshot.creatures[(x - 1) + y * size];
            let open_right = x + 1 == size || !snapshot.creatures[(x + 1) + y * size];

            if !open_above && !open_below && !open_left && !open_right {
                continue;
            }

            let px = offset_x + (x as u32 * cell) as i64;
            let py = offset_y + (y as u32 * cell) as i64;

            if open_above {
                canvas.fill_rect(px, py, cell, 1, color);
            }
            if open_below {
                canvas.fill_rect(px, py + cell as i64 - 1, cell, 1, color);
            }
            if open_left {
                canvas.fill_rect(px, py, 1, cell, color);
            }
            if open_right {
                canvas.fill_rect(px + cell as i64 - 1, py, 1, cell, color);
            }
        }
    }
}

/// Рамка банки и внутренняя тень: банка читается как сосуд.
fn draw_jar_frame(canvas: &mut Canvas, x: i64, y: i64, grid: u32, palette: &Palette) {
    // Тень внутри поля: отделяет содержимое от стекла.
    canvas.fill_rect(x, y, grid, JAR_SHADOW_THICKNESS, palette.jar_shadow);
    canvas.fill_rect(x, y, JAR_SHADOW_THICKNESS, grid, palette.jar_shadow);

    // Рамка.
    canvas.fill_rect(x, y, grid, JAR_FRAME_THICKNESS, palette.jar_frame);
    canvas.fill_rect(
        x,
        y + grid as i64 - JAR_FRAME_THICKNESS as i64,
        grid,
        JAR_FRAME_THICKNESS,
        palette.jar_frame,
    );
    canvas.fill_rect(x, y, JAR_FRAME_THICKNESS, grid, palette.jar_frame);
    canvas.fill_rect(
        x + grid as i64 - JAR_FRAME_THICKNESS as i64,
        y,
        JAR_FRAME_THICKNESS,
        grid,
        palette.jar_frame,
    );
}

/// Курсор: четыре уголка, чтобы не перекрывать содержимое клетки.
fn draw_cursor(
    canvas: &mut Canvas,
    context: &RenderContext,
    offset_x: i64,
    offset_y: i64,
    cell: u32,
) {
    let (x, y) = context.cursor;
    let index = x + y * context.snapshot.size;
    let under = cell_color(context.snapshot, index, context.view, context.palette);
    let palette = context.palette;
    let color = if luma(under) > 140 {
        palette.cursor_dark
    } else {
        palette.cursor_light
    };

    let arm = (cell / 3).max(3).min(cell);
    let thickness = if cell < 6 { 1 } else { 2 };
    let px = offset_x + (x as u32 * cell) as i64;
    let py = offset_y + (y as u32 * cell) as i64;

    // Левый верхний угол.
    canvas.fill_rect(px, py, arm, thickness, color);
    canvas.fill_rect(px, py, thickness, arm, color);
    // Правый верхний.
    canvas.fill_rect(px + cell as i64 - arm as i64, py, arm, thickness, color);
    canvas.fill_rect(
        px + cell as i64 - thickness as i64,
        py,
        thickness,
        arm,
        color,
    );
    // Левый нижний.
    canvas.fill_rect(
        px,
        py + cell as i64 - thickness as i64,
        arm,
        thickness,
        color,
    );
    canvas.fill_rect(px, py + cell as i64 - arm as i64, thickness, arm, color);
    // Правый нижний.
    canvas.fill_rect(
        px + cell as i64 - arm as i64,
        py + cell as i64 - thickness as i64,
        arm,
        thickness,
        color,
    );
    canvas.fill_rect(
        px + cell as i64 - thickness as i64,
        py + cell as i64 - arm as i64,
        thickness,
        arm,
        color,
    );
}

/// Оверлей освещенности.
///
/// Свет считается по той же формуле, что и в симуляции, но в дробных клетках,
/// поэтому картинка совпадает с областями фотосинтеза и остается гладкой.
/// Яркость клеток падает в темноте, а на освещенных краях появляется
/// теплый оттенок солнечного света.
fn draw_light_overlay(
    canvas: &mut Canvas,
    offset_x: i64,
    offset_y: i64,
    grid: u32,
    cell: u32,
    size: usize,
) {
    let reach_px = light_reach(size) as u32 * cell;
    let table = build_light_table(reach_px, cell, size);
    // Палитра света готовится один раз на кадр, а не на каждый пиксель.
    let palette: Vec<u32> = (0..=255u8).map(light_color).collect();

    let x0 = offset_x.max(0).min(canvas.width as i64) as u32;
    let y0 = offset_y.max(0).min(canvas.height as i64) as u32;
    let x1 = (offset_x + grid as i64).max(0).min(canvas.width as i64) as u32;
    let y1 = (offset_y + grid as i64).max(0).min(canvas.height as i64) as u32;

    let right = offset_x + grid as i64 - 1;
    let bottom = offset_y + grid as i64 - 1;

    for py in y0..y1 {
        let dy = (py as i64 - offset_y).min(bottom - py as i64).max(0);

        for px in x0..x1 {
            let dx = (px as i64 - offset_x).min(right - px as i64).max(0);
            let distance = dx.min(dy) as usize;
            let intensity = table.get(distance).copied().unwrap_or(0);

            // В темноте клетки почти не видны, на свету — полная яркость.
            let factor = DARK_FACTOR + mul255(intensity as u32, 255 - DARK_FACTOR);
            let warm = mul255(intensity as u32, WARM_STRENGTH);
            let index = (py * canvas.width + px) as usize;
            let lit = scale(canvas.buffer[index], factor);

            canvas.buffer[index] = mix(lit, palette[intensity as usize], warm);
        }
    }
}

/// Таблица освещенности: интенсивность 0..=255 по расстоянию до края в пикселях.
fn build_light_table(reach_px: u32, cell: u32, size: usize) -> Vec<u8> {
    let reach_cells = light_reach(size) as f32;
    let cell = cell.max(1) as f32;

    (0..=reach_px)
        .map(|distance| {
            let distance_cells = distance as f32 / cell;
            if distance_cells >= reach_cells {
                return 0;
            }
            let level = 1.0 - distance_cells / reach_cells;
            (level * 255.0).round() as u8
        })
        .collect()
}

/// Панель с основными показателями.
fn draw_hud(canvas: &mut Canvas, context: &RenderContext) {
    let lines = [context.hud_top.as_str(), context.hud_bottom.as_str()];
    let palette = context.palette;
    draw_panel(
        canvas,
        PANEL_MARGIN as i64,
        PANEL_MARGIN as i64,
        &lines,
        palette,
    );
}

/// Панель инспектора: что находится в выбранной клетке.
fn draw_inspector(canvas: &mut Canvas, context: &RenderContext) {
    let info = &context.info;
    let mut lines = vec![format!("cell {},{}", info.x, info.y)];

    if info.alive {
        lines.push(format!("energy {}  minerals {}", info.energy, info.eaten));
        lines.push(format!(
            "age {}  action {}",
            info.age,
            action_label(info.action)
        ));
        lines.push(format!("dna {}", mix_label(info.mix)));
    } else {
        lines.push("empty".to_string());
        lines.push(format!("minerals {}", info.minerals));
    }

    let panel_height = lines.len() as u32 * FONT_HEIGHT + PANEL_PADDING * 2;
    let y = canvas.height as i64 - panel_height as i64 - PANEL_MARGIN as i64;
    let references: Vec<&str> = lines.iter().map(String::as_str).collect();
    draw_panel(canvas, PANEL_MARGIN as i64, y, &references, context.palette);
}

/// Легенда шкалы для режимов-тепкарт.
fn draw_legend(canvas: &mut Canvas, context: &RenderContext) {
    let (stops, max_value) = match context.view {
        ViewMode::Minerals => (&MINERAL_RAMP, context.snapshot.max_minerals),
        ViewMode::Energy => (&ENERGY_RAMP, context.snapshot.max_energy),
        _ => return,
    };

    let x = canvas.width as i64 - LEGEND_WIDTH as i64 - PANEL_MARGIN as i64;
    let y =
        canvas.height as i64 - LEGEND_HEIGHT as i64 - PANEL_MARGIN as i64 - FONT_HEIGHT as i64 - 2;

    for i in 0..LEGEND_WIDTH {
        let t = i * 255 / (LEGEND_WIDTH - 1);
        canvas.fill_rect(x + i as i64, y, 1, LEGEND_HEIGHT, ramp(t, stops));
    }

    let palette = context.palette;
    draw_text(
        canvas,
        x,
        y + LEGEND_HEIGHT as i64 + 2,
        "0",
        palette.panel_text,
    );

    let max_label = format_count(max_value);
    let label_x = x + LEGEND_WIDTH as i64 - max_label.len() as i64 * TEXT_ADVANCE as i64;
    draw_text(
        canvas,
        label_x,
        y + LEGEND_HEIGHT as i64 + 2,
        &max_label,
        palette.panel_text,
    );
}

/// Мини-карта: вся банка целиком и рамка текущего вида.
fn draw_minimap(
    canvas: &mut Canvas,
    context: &RenderContext,
    offset_x: i64,
    offset_y: i64,
    cell: u32,
) {
    let snapshot = context.snapshot;
    let size = snapshot.size;
    if size == 0 || MINIMAP_SIZE == 0 {
        return;
    }

    let x = canvas.width as i64 - MINIMAP_SIZE as i64 - PANEL_MARGIN as i64;
    let y = PANEL_MARGIN as i64;

    // Фон мини-карты и точки существ.
    canvas.fill_rect(x, y, MINIMAP_SIZE, MINIMAP_SIZE, context.palette.jar_bg);

    for cy in 0..size {
        for cx in 0..size {
            if snapshot.creatures[cx + cy * size] {
                canvas.fill_rect(x + cx as i64, y + cy as i64, 1, 1, context.palette.creature);
            }
        }
    }

    // Рамка текущего вида: какие клетки поля попали в окно.
    let cell_i = cell.max(1) as i64;
    let left = ((-offset_x) / cell_i).clamp(0, size as i64);
    let top = ((-offset_y) / cell_i).clamp(0, size as i64);
    let right = ((canvas.width as i64 - offset_x) / cell_i).clamp(0, size as i64);
    let bottom = ((canvas.height as i64 - offset_y) / cell_i).clamp(0, size as i64);

    let view_x = x + left;
    let view_y = y + top;
    let view_width = (right - left).max(1);
    let view_height = (bottom - top).max(1);

    for edge in 0..view_width {
        canvas.fill_rect(view_x + edge, view_y, 1, 1, context.palette.cursor_dark);
        canvas.fill_rect(
            view_x + edge,
            view_y + view_height - 1,
            1,
            1,
            context.palette.cursor_dark,
        );
    }
    for edge in 0..view_height {
        canvas.fill_rect(view_x, view_y + edge, 1, 1, context.palette.cursor_dark);
        canvas.fill_rect(
            view_x + view_width - 1,
            view_y + edge,
            1,
            1,
            context.palette.cursor_dark,
        );
    }

    // Внешняя рамка мини-карты.
    canvas.fill_rect(
        x - 1,
        y - 1,
        MINIMAP_SIZE + 2,
        1,
        context.palette.panel_border,
    );
    canvas.fill_rect(
        x - 1,
        y + MINIMAP_SIZE as i64,
        MINIMAP_SIZE + 2,
        1,
        context.palette.panel_border,
    );
    canvas.fill_rect(
        x - 1,
        y - 1,
        1,
        MINIMAP_SIZE + 2,
        context.palette.panel_border,
    );
    canvas.fill_rect(
        x + MINIMAP_SIZE as i64,
        y - 1,
        1,
        MINIMAP_SIZE + 2,
        context.palette.panel_border,
    );
}

/// Панель с текстом на светлом фоне.
fn draw_panel(canvas: &mut Canvas, x: i64, y: i64, lines: &[&str], palette: &Palette) {
    let text_width = lines
        .iter()
        .map(|line| line.len() as u32 * TEXT_ADVANCE)
        .max()
        .unwrap_or(0);
    let panel_width = text_width + PANEL_PADDING * 2;
    let panel_height = lines.len() as u32 * FONT_HEIGHT + PANEL_PADDING * 2;

    canvas.fill_rect(x, y, panel_width, panel_height, palette.panel_bg);

    // Рамка панели.
    canvas.fill_rect(x, y, panel_width, 1, palette.panel_border);
    canvas.fill_rect(
        x,
        y + panel_height as i64 - 1,
        panel_width,
        1,
        palette.panel_border,
    );
    canvas.fill_rect(x, y, 1, panel_height, palette.panel_border);
    canvas.fill_rect(
        x + panel_width as i64 - 1,
        y,
        1,
        panel_height,
        palette.panel_border,
    );

    for (index, line) in lines.iter().enumerate() {
        draw_text(
            canvas,
            x + PANEL_PADDING as i64,
            y + PANEL_PADDING as i64 + index as i64 * FONT_HEIGHT as i64,
            line,
            palette.panel_text,
        );
    }
}

/// Рисует строку текста шрифтом 8x8.
fn draw_text(canvas: &mut Canvas, x: i64, y: i64, text: &str, color: u32) {
    let mut cursor_x = x;

    for character in text.chars() {
        let code = character as usize;
        if code < 128 {
            let glyph = font8x8::legacy::BASIC_LEGACY[code];
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..FONT_WIDTH {
                    if bits & (1 << column) != 0 {
                        canvas.pixel(cursor_x + column as i64, y + row as i64, color);
                    }
                }
            }
        }

        cursor_x += TEXT_ADVANCE as i64;
    }
}

/// Подпись типа действия.
fn action_label(action: Option<CreatureAction>) -> &'static str {
    match action {
        Some(CreatureAction::PhotoGrow) => "photo",
        Some(CreatureAction::EatMinerals) => "eat",
        Some(CreatureAction::Split) => "split",
        Some(CreatureAction::Jump(_)) => "jump",
        None => "-",
    }
}

/// Подпись состава генома: доли типов генов в процентах.
fn mix_label(mix: Option<[u8; 4]>) -> String {
    let Some(mix) = mix else {
        return "-".to_string();
    };

    let names = ["photo", "eat", "split", "jump"];
    let parts: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(index, name)| format!("{name} {}%", mix[index] as u32 * 100 / 255))
        .collect();

    parts.join(" ")
}

/// Короткая запись больших чисел: 42000 -> 42.0k.
fn format_count(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}m", value as f32 / 1_000_000.0)
    } else if value >= 10_000 {
        format!("{:.1}k", value as f32 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// Цвет по рампе: `t` в диапазоне 0..=255.
fn ramp(t: u32, stops: &[(u32, (u32, u32, u32)); 5]) -> u32 {
    let t = t.min(255);

    for pair in stops.windows(2) {
        let (from_value, from_color) = pair[0];
        let (to_value, to_color) = pair[1];

        if t <= to_value {
            let span = (to_value - from_value).max(1);
            let k = t.saturating_sub(from_value) * 255 / span;
            let mix_channel = |from: u32, to: u32| (from * (255 - k) + to * k) / 255;

            return rgb(
                mix_channel(from_color.0, to_color.0) as u8,
                mix_channel(from_color.1, to_color.1) as u8,
                mix_channel(from_color.2, to_color.2) as u8,
            );
        }
    }

    let last = stops[stops.len() - 1].1;
    rgb(last.0 as u8, last.1 as u8, last.2 as u8)
}

/// Смешивает два цвета: t = 0 — base, t = 255 — overlay.
///
/// Деление на 255 заменено умножением и сдвигом: значения не округляются
/// грубо, а в горячем цикле нет делений.
fn mix(base: u32, overlay: u32, t: u32) -> u32 {
    let t = t.min(255);
    let inverse = 255 - t;
    let mix_channel = |shift: u32| {
        let base_channel = (base >> shift) & 0xFF;
        let overlay_channel = (overlay >> shift) & 0xFF;
        ((base_channel * inverse + overlay_channel * t) * 257 + 257) >> 16
    };

    (mix_channel(16) << 16) | (mix_channel(8) << 8) | mix_channel(0)
}

/// Светлота цвета в диапазоне 0..=255.
fn luma(color: u32) -> u32 {
    let r = (color >> 16) & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = color & 0xFF;
    (r * 299 + g * 587 + b * 114) / 1000
}

/// Умножает цвет на коэффициент яркости 0..=255.
fn scale(color: u32, factor: u32) -> u32 {
    let factor = factor.min(255);
    let channel = |shift: u32| ((((color >> shift) & 0xFF) * factor) * 257 + 257) >> 16;
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// Умножение без деления: `value * factor / 255`.
fn mul255(value: u32, factor: u32) -> u32 {
    (value * factor * 257 + 257) >> 16
}

/// Красивый градиент освещенности: от почти черной тьмы в центре
/// через синий и теплый оранжевый к мягкому солнечному свету у края.
fn light_color(intensity: u8) -> u32 {
    const STOPS: [(u32, (u32, u32, u32)); 4] = [
        (0, (8, 10, 24)),
        (90, (36, 50, 112)),
        (180, (212, 116, 52)),
        (255, (255, 244, 190)),
    ];

    let value = intensity as u32;
    for pair in STOPS.windows(2) {
        let (from_value, from_color) = pair[0];
        let (to_value, to_color) = pair[1];

        if value <= to_value {
            let t = (value - from_value) * 255 / (to_value - from_value);
            let mix = |from: u32, to: u32| ((from * (255 - t) + to * t) / 255) as u8;
            return rgb(
                mix(from_color.0, to_color.0),
                mix(from_color.1, to_color.1),
                mix(from_color.2, to_color.2),
            );
        }
    }

    rgb(255, 244, 190)
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Расстояние от клетки до ближайшего края Jar.
fn distance_to_edge(x: usize, y: usize, size: usize) -> usize {
    let last = size.saturating_sub(1);
    x.min(y)
        .min(last.saturating_sub(x))
        .min(last.saturating_sub(y))
}

/// Половина расстояния от края до центра — дальше света нет.
fn light_reach(size: usize) -> usize {
    (size / 4).max(1)
}

/// Сколько энергии дает фотосинтез в клетке `(x, y)`.
///
/// Свет идет от краев Jar к центру и полностью затухает на половине пути
/// к центру. Чем ближе клетка к краю, тем больше энергии она получает.
fn light_level(x: usize, y: usize, size: usize) -> usize {
    let reach = light_reach(size);
    let distance = distance_to_edge(x, y, size);

    if distance >= reach {
        return 0;
    }

    MAX_PHOTOSYNTHESIS_ENERGY - MAX_PHOTOSYNTHESIS_ENERGY * distance / reach
}

/// Прибавляет `delta` к указателю ДНК, не позволяя ему стать отрицательным.
fn apply_jump(pointer: usize, delta: isize) -> usize {
    let next = pointer as isize + delta;
    if next < 0 { 0 } else { next as usize }
}

/// Оттенок гена в градусах HSV.
fn gene_hue(action: &CreatureAction) -> f32 {
    match action {
        CreatureAction::PhotoGrow => 120.0,
        CreatureAction::EatMinerals => 240.0,
        CreatureAction::Split => 0.0,
        CreatureAction::Jump(_) => 300.0,
    }
}

/// Цвет генома: взвешенная циркулярная средняя оттенков генов.
///
/// Насыщенность всегда 100%, поэтому смесь любых генов остается яркой
/// и никогда не превращается в серый цвет.
pub(crate) fn genome_color(genome: &CreatureGenome) -> u32 {
    let mut sin_sum = 0.0f32;
    let mut cos_sum = 0.0f32;

    for (i, action) in genome.line.iter().enumerate() {
        let weight = (i + 1) as f32;
        let radians = gene_hue(action).to_radians();
        sin_sum += weight * radians.sin();
        cos_sum += weight * radians.cos();
    }

    // Если противоположные оттенки почти погасили друг друга,
    // берем оттенок первого гена. Насыщенность все равно останется 100%.
    let hue = if sin_sum * sin_sum + cos_sum * cos_sum < 1e-6 {
        gene_hue(&genome.line[0])
    } else {
        sin_sum.atan2(cos_sum).to_degrees().rem_euclid(360.0)
    };

    hsv_to_rgb(hue, 1.0, 0.85)
}

/// Переводит HSV в packed RGB.
fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let h = hue.rem_euclid(360.0) / 60.0;
    let c = value * saturation;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;

    rgb(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

pub struct Jar {
    /// Ширина и высота пространства
    pub size: usize,

    /// Номер тика, используется для рандома
    pub tick: usize,

    /// Слой существ
    pub creatures: Vec<Option<Creature>>,
    /// Слой минералов
    pub minerals: Vec<usize>,
    /// Генератор случайности для мутаций и выбора направления деления.
    pub rng: Rng,
}
impl Jar {
    /// Jar квадратная
    pub fn new(size: usize) -> Self {
        let creatures = vec![None; size * size];
        let minerals = vec![100usize; size * size];

        Self {
            size,
            tick: 0,
            creatures,
            minerals,
            rng: Rng::from_time(),
        }
    }

    /// Создает снимок состояния мира для отрисовки.
    pub fn snapshot(&self) -> JarSnapshot {
        let mut max_minerals = 0;
        let mut max_energy = 0;
        let mut population = 0;

        for (index, cell) in self.creatures.iter().enumerate() {
            max_minerals = max_minerals.max(self.minerals[index]);
            if let Some(creature) = cell {
                max_energy = max_energy.max(creature.energy);
                population += 1;
            }
        }

        JarSnapshot {
            size: self.size,
            tick: self.tick,
            minerals: self.minerals.clone(),
            creatures: self.creatures.iter().map(|cell| cell.is_some()).collect(),
            energies: self
                .creatures
                .iter()
                .map(|cell| cell.as_ref().map_or(0, |creature| creature.energy))
                .collect(),
            genome_colors: self
                .creatures
                .iter()
                .map(|cell| cell.as_ref().map(|creature| genome_color(&creature.genome)))
                .collect(),
            actions: self
                .creatures
                .iter()
                .map(|cell| cell.as_ref().and_then(|creature| creature.last_action))
                .collect(),
            genome_mix: self
                .creatures
                .iter()
                .map(|cell| cell.as_ref().map(|creature| genome_mix(&creature.genome)))
                .collect(),
            max_minerals,
            max_energy,
            population,
        }
    }

    pub fn simulation_step(&mut self) {
        self.tick += 1;
        for i in 0..(self.size * self.size) {
            if let Some(mut creature) = self.creatures[i].clone() {
                // Если в клетке есть существо, берем его копию
                creature.age += 1;

                if creature.age >= CREATURE_LIFETIME_TICKS {
                    // Клетка прожила свой срок - умирает, а все ее минералы
                    // возвращаются обратно на карту
                    self.minerals[i] += creature.minerals;
                    self.creatures[i] = None;
                    continue;
                }

                let action = creature.genome.get_action();
                // Запоминаем, что именно сделала клетка: по этому видно цикл ДНК.
                creature.last_action = Some(action);

                match action {
                    CreatureAction::Split => {
                        // Для деления нужно минимум 100 энергии и минимум 10 минералов
                        if creature.energy >= 100 && creature.minerals >= 10 {
                            let i_x = i % self.size;
                            let i_y = i / self.size;

                            // Проверить рядомстоящие позиции. Если рядом свободно - добавить эту  позицию в массив
                            let nears: Vec<usize> = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)]
                                .iter()
                                .filter_map(|&(dx, dy)| {
                                    let x = i_x.checked_add_signed(dx)?;
                                    let y = i_y.checked_add_signed(dy)?;
                                    if x >= self.size || y >= self.size {
                                        return None; // Вышли за границы поля
                                    }
                                    let j = x + y * self.size;
                                    if self.creatures[j].is_none() {
                                        Some(j)
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            if nears.is_empty() {
                                // Некуда делиться - клетка умирает, а все ее минералы
                                // возвращаются обратно на карту
                                self.minerals[i] += creature.minerals;
                                self.creatures[i] = None;
                            } else {
                                // Есть куда положить дочернюю клетку
                                let (new_0, new_1) = creature.split(&mut self.rng);
                                let j = nears[self.tick % nears.len()]; // Выбор места дочерней клетки
                                self.creatures[i] = Some(new_0); // На месте материнской клетки
                                self.creatures[j] = Some(new_1);
                            }
                        } else {
                            self.creatures[i] = Some(creature);
                        }
                    }
                    CreatureAction::PhotoGrow => {
                        let x = i % self.size;
                        let y = i / self.size;
                        creature.energy += light_level(x, y, self.size);
                        self.creatures[i] = Some(creature);
                    }
                    CreatureAction::EatMinerals => {
                        // Есть можно, только если хватает энергии и минералов.
                        if self.minerals[i] > 0 && creature.energy >= EAT_MINERALS_ENERGY_COST {
                            creature.energy -= EAT_MINERALS_ENERGY_COST;
                            creature.minerals += 1;
                            self.minerals[i] -= 1;
                        }
                        self.creatures[i] = Some(creature);
                    }
                    CreatureAction::Jump(delta) => {
                        // get_action уже сдвинул указатель на +1, поэтому здесь
                        // получается итоговый сдвиг +1+X.
                        creature.genome.line_pointer =
                            apply_jump(creature.genome.line_pointer, delta);
                        self.creatures[i] = Some(creature);
                    }
                }
            }
        }
    }

    pub fn print(&self) {
        for y in 0..self.size {
            for x in 0..self.size {
                if self.creatures[x + y * self.size].is_none() {
                    print!("·");
                } else {
                    print!("█");
                }
            }
            println!();
        }
    }
}
#[derive(Clone, Debug)]
pub struct Creature {
    /// Пока что введем абстракцию, что ген 0 - это фотосинтезировать, а при достижении энергии 100 клетка делится в случайную сторону
    pub genome: CreatureGenome,

    pub energy: usize,
    pub minerals: usize,
    /// Возраст клетки в тиках.
    pub age: usize,
    /// Последнее выполненное действие: нужно для режима просмотра `action`.
    pub last_action: Option<CreatureAction>,
}
impl Creature {
    /// Делит клетку на две.
    ///
    /// Каждая дочерняя клетка получает по 40% изначальной энергии, остаток теряется
    /// на само деление. Минералы делятся поровну без потерь: неделимый остаток
    /// достается материнской клетке. Обе дочерние клетки рождаются с нулевым возрастом.
    ///
    /// Каждая дочерняя клетка независимо может получить мутацию генома.
    pub fn split(&self, rng: &mut Rng) -> (Self, Self) {
        let energy = self.energy;
        let minerals = self.minerals;

        let e_0 = energy * 40 / 100;
        let e_1 = energy * 40 / 100;

        let m_0 = (minerals / 2) + (minerals % 2);
        let m_1 = minerals - m_0;

        let mut c_0 = Creature {
            genome: self.genome.clone(),
            energy: e_0,
            minerals: m_0,
            age: 0,
            last_action: None,
        };
        let mut c_1 = Creature {
            genome: self.genome.clone(),
            energy: e_1,
            minerals: m_1,
            age: 0,
            last_action: None,
        };

        c_0.genome.line_pointer = 0;
        c_1.genome.line_pointer = 0;

        if rng.chance_percent(MUTATION_CHANCE_PERCENT) {
            c_0.genome.mutate(rng);
        }
        if rng.chance_percent(MUTATION_CHANCE_PERCENT) {
            c_1.genome.mutate(rng);
        }

        (c_0, c_1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureAction {
    Split,
    PhotoGrow,
    EatMinerals,
    /// Сдвинуть указатель ДНК на X (вдобавок к обычному +1).
    Jump(isize),
}
#[derive(Clone, Debug)]
pub struct CreatureGenome {
    /// Нить "ДНК"
    pub line: Vec<CreatureAction>,
    pub line_pointer: usize,
}
impl CreatureGenome {
    /// Стандартный геном из 16 генов.
    ///
    /// Базовый паттерн `[PhotoGrow, PhotoGrow, EatMinerals, Split]`
    /// повторяется четыре раза.
    pub fn standard() -> Self {
        let pattern = [
            CreatureAction::PhotoGrow,
            CreatureAction::PhotoGrow,
            CreatureAction::EatMinerals,
            CreatureAction::Split,
        ];

        Self {
            line: pattern.iter().cycle().take(16).copied().collect(),
            line_pointer: 0,
        }
    }

    /// Создает геном из готовой нити ДНК.
    pub fn from_line(line: Vec<CreatureAction>) -> Self {
        Self {
            line,
            line_pointer: 0,
        }
    }

    /// Мутирует геном одним случайным способом:
    /// вставка гена, удаление гена, замена гена или изменение значения Jump.
    pub fn mutate(&mut self, rng: &mut Rng) {
        if self.line.is_empty() {
            self.line.push(random_action(rng));
            return;
        }

        match rng.below(4) {
            0 => {
                // Вставка нового гена в случайную позицию.
                let index = rng.below(self.line.len() + 1);
                self.line.insert(index, random_action(rng));
            }
            1 => {
                // Изменение значения Jump; если Jump нет — обычная замена гена.
                if !self.mutate_jump_value(rng) {
                    self.replace_gene(rng);
                }
            }
            2 => {
                // Удаление гена, но геном никогда не остается пустым.
                if self.line.len() > 1 {
                    let index = rng.below(self.line.len());
                    self.line.remove(index);
                } else {
                    self.replace_gene(rng);
                }
            }
            _ => self.replace_gene(rng),
        }
    }

    /// Заменяет случайный ген на случайный другой.
    fn replace_gene(&mut self, rng: &mut Rng) {
        let index = rng.below(self.line.len());
        self.line[index] = random_action(rng);
    }

    /// Изменяет значение случайного гена Jump. Возвращает `false`, если Jump нет.
    fn mutate_jump_value(&mut self, rng: &mut Rng) -> bool {
        let jump_positions: Vec<usize> = self
            .line
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                matches!(action, CreatureAction::Jump(_)).then_some(index)
            })
            .collect();

        if jump_positions.is_empty() {
            return false;
        }

        let index = jump_positions[rng.below(jump_positions.len())];
        if let CreatureAction::Jump(value) = &mut self.line[index] {
            *value += random_jump_delta(rng);
        }

        true
    }

    /// Выдает текущее действие клетки, сдвигает указатель действия на следующий
    pub fn get_action(&mut self) -> CreatureAction {
        let now_pointer = self.line_pointer % self.line.len();
        self.line_pointer += 1;
        self.line[now_pointer]
    }
}

pub struct JarSnapshot {
    /// Ширина и высота снимка
    pub size: usize,
    /// Номер тика, к которому относится снимок
    pub tick: usize,
    pub minerals: Vec<usize>,
    pub creatures: Vec<bool>,
    /// Энергия существ, 0 для пустых клеток.
    pub energies: Vec<usize>,
    /// Цвет генома существа, `None` для пустых клеток.
    pub genome_colors: Vec<Option<u32>>,
    /// Последнее выполненное действие клетки, `None` для пустых клеток.
    pub actions: Vec<Option<CreatureAction>>,
    /// Доли генов каждого типа в геноме, `None` для пустых клеток.
    pub genome_mix: Vec<Option<[u8; 4]>>,
    /// Максимум минералов в поле: верхняя граница шкалы.
    pub max_minerals: usize,
    /// Максимум энергии в поле: верхняя граница шкалы.
    pub max_energy: usize,
    /// Сколько существ в поле.
    pub population: usize,
}

/// Доли генов каждого типа в геноме: [PhotoGrow, EatMinerals, Split, Jump].
///
/// Сумма долей всегда равна 255, поэтому полоска состава генома
/// заполняет клетку целиком.
fn genome_mix(genome: &CreatureGenome) -> [u8; 4] {
    let mut counts = [0u32; 4];
    for action in &genome.line {
        counts[action_index(*action)] += 1;
    }

    let total: u32 = counts.iter().sum();
    if total == 0 {
        return [255, 0, 0, 0];
    }

    let mut mix = [0u8; 4];
    let mut sum = 0u32;
    for index in 0..3 {
        mix[index] = (counts[index] * 255 / total) as u8;
        sum += mix[index] as u32;
    }
    mix[3] = (255 - sum).min(255) as u8;

    mix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_genome_has_16_genes() {
        assert_eq!(CreatureGenome::standard().line.len(), 16);
    }

    #[test]
    fn jump_adds_to_pointer_after_regular_step() {
        let mut genome =
            CreatureGenome::from_line(vec![CreatureAction::Jump(2), CreatureAction::PhotoGrow]);

        let action = genome.get_action();
        if let CreatureAction::Jump(delta) = action {
            genome.line_pointer = apply_jump(genome.line_pointer, delta);
        }

        // get_action дал +1, Jump(2) добавил еще +2.
        assert_eq!(genome.line_pointer, 3);
    }

    #[test]
    fn negative_jump_does_not_underflow() {
        assert_eq!(apply_jump(1, -5), 0);
    }

    #[test]
    fn initial_genomes_have_distinct_colors() {
        let colors = [genome_top(), genome_right(), genome_bottom(), genome_left()]
            .map(|genome| genome_color(&genome));

        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "цвета вариантов {i} и {j} совпали");
            }
        }
    }

    #[test]
    fn spawn_places_creatures_at_positions() {
        let mut jar = Jar::new(20);
        spawn(&mut jar, genome_top(), &[(9, 2), (10, 2), (11, 2)]);

        for (x, y) in [(9, 2), (10, 2), (11, 2)] {
            assert!(jar.creatures[x + y * jar.size].is_some());
        }
    }

    #[test]
    fn initial_population_survives_and_grows() {
        let mut jar = seeded_jar();
        jar.rng = Rng::new(0xC0FFEE);
        let initial = jar.creatures.iter().filter(|cell| cell.is_some()).count();
        assert_eq!(initial, 12);

        for _ in 0..300 {
            jar.simulation_step();
        }

        let alive = jar.creatures.iter().filter(|cell| cell.is_some()).count();
        assert!(alive > initial, "популяция вымерла: {alive} <= {initial}");
    }

    #[test]
    fn rng_is_deterministic() {
        let mut first = Rng::new(42);
        let mut second = Rng::new(42);

        for _ in 0..16 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
    }

    #[test]
    fn rng_zero_seed_is_replaced() {
        let mut rng = Rng::new(0);
        assert_ne!(rng.next_u64(), 0);
    }

    #[test]
    fn mutations_never_leave_genome_empty() {
        let mut rng = Rng::new(7);
        let mut genome = CreatureGenome::from_line(vec![CreatureAction::PhotoGrow]);

        for _ in 0..1000 {
            genome.mutate(&mut rng);
            assert!(!genome.line.is_empty());
        }
    }

    #[test]
    fn jump_value_mutation_changes_value() {
        let mut rng = Rng::new(123);
        let mut genome = CreatureGenome::from_line(vec![CreatureAction::Jump(5)]);

        assert!(genome.mutate_jump_value(&mut rng));
        assert_ne!(genome.line[0], CreatureAction::Jump(5));
    }

    #[test]
    fn mutations_eventually_change_genome() {
        let mut rng = Rng::new(99);
        let mut genome = CreatureGenome::standard();
        let original = genome.clone();

        for _ in 0..100 {
            genome.mutate(&mut rng);
            if genome.line != original.line {
                break;
            }
        }

        assert_ne!(genome.line, original.line);
    }

    #[test]
    fn genome_colors_are_never_gray() {
        let genes = [
            CreatureAction::PhotoGrow,
            CreatureAction::EatMinerals,
            CreatureAction::Split,
            CreatureAction::Jump(1),
        ];

        // Перебираем периодические геномы с периодом 1..=4.
        for period in 1..=4 {
            let combinations = 4usize.pow(period as u32);
            for mask in 0..combinations {
                let mut rest = mask;
                let mut pattern = Vec::with_capacity(period);
                for _ in 0..period {
                    pattern.push(genes[rest % genes.len()]);
                    rest /= genes.len();
                }

                let line = (0..16).map(|i| pattern[i % period]).collect();
                let color = genome_color(&CreatureGenome::from_line(line));
                let r = (color >> 16) & 0xFF;
                let g = (color >> 8) & 0xFF;
                let b = color & 0xFF;

                assert!(
                    !(r == g && g == b),
                    "геном дал серый цвет: pattern={pattern:?}, color={color:#08X}"
                );
            }
        }
    }

    #[test]
    fn fit_cell_size_fits_the_field() {
        let size = 100;
        let cell = fit_cell_size(800, 800, size);
        assert!(cell * size as u32 <= 800, "поле {size} не влезает в окно");
        assert_eq!(fit_cell_size(800, 800, size), 8);
    }

    #[test]
    fn field_is_centered_when_it_fits() {
        let (offset_x, offset_y) = field_offset((50, 50), 100, 8, 800, 800);
        assert_eq!(offset_x, 0);
        assert_eq!(offset_y, 0);

        // В окне 100x100 поле 800 пикселей центрируется с полями по 50.
        let (offset_x, offset_y) = field_offset((10, 10), 100, 4, 800, 800);
        assert_eq!(offset_x, 200);
        assert_eq!(offset_y, 200);
    }

    #[test]
    fn cursor_stays_in_the_center_when_the_field_does_not_fit() {
        let (offset_x, offset_y) = field_offset((10, 20), 100, 16, 800, 800);
        let cell = 16i64;
        let cursor_center_x = 10 * cell + cell / 2;
        let cursor_center_y = 20 * cell + cell / 2;

        assert_eq!(offset_x + cursor_center_x, 400);
        assert_eq!(offset_y + cursor_center_y, 400);
    }

    #[test]
    fn genome_mix_shares_add_up_to_full_cell() {
        let genome = CreatureGenome::standard();
        let mix = genome_mix(&genome);
        assert_eq!(mix.iter().map(|share| *share as u32).sum::<u32>(), 255);
    }

    #[test]
    fn mutation_changes_genome_mix() {
        let mut genome = CreatureGenome::standard();
        let before = genome_mix(&genome);

        genome.line[0] = CreatureAction::Jump(1);
        let after = genome_mix(&genome);

        assert_ne!(before, after, "мутация не изменила состав генома");
        assert_eq!(after.iter().map(|share| *share as u32).sum::<u32>(), 255);
        assert!(after[3] > before[3], "доля прыжков не выросла");
    }

    #[test]
    fn ramp_spans_from_light_to_dark() {
        let light = ramp(0, &MINERAL_RAMP);
        let dark = ramp(255, &MINERAL_RAMP);

        assert_eq!(light, rgb(207, 227, 245));
        assert_eq!(dark, rgb(18, 63, 126));
        assert!(luma(light) > luma(dark), "шкала не упорядочена по светлоте");
    }

    #[test]
    fn mix_and_scale_keep_the_endpoints() {
        let base = rgb(10, 20, 30);
        let overlay = rgb(200, 100, 50);

        assert_eq!(mix(base, overlay, 0), base);
        assert_eq!(mix(base, overlay, 255), overlay);
        assert_eq!(scale(base, 255), base);
        assert_eq!(scale(base, 0), rgb(0, 0, 0));
    }

    #[test]
    fn cursor_color_contrasts_with_the_cell_under_it() {
        let palette = Palette::new();

        for cell_color in [
            palette.jar_bg,
            palette.creature,
            palette.action_colors[0],
            palette.action_colors[1],
            palette.action_colors[2],
            palette.action_colors[3],
        ] {
            let cursor = if luma(cell_color) > 140 {
                palette.cursor_dark
            } else {
                palette.cursor_light
            };

            assert!(
                (luma(cursor) as i64 - luma(cell_color) as i64).abs() > 60,
                "курсор {cursor:#08X} не виден на клетке {cell_color:#08X}"
            );
        }
    }

    #[test]
    fn light_falls_off_towards_the_center() {
        let reach_px = 25 * 8;
        let table = build_light_table(reach_px, 8, 100);

        assert_eq!(table[0], 255, "у самого края должно быть полное освещение");
        assert_eq!(
            *table.last().unwrap(),
            0,
            "в конце зоны света должно быть темно"
        );

        for distance in 1..table.len() {
            assert!(
                table[distance] <= table[distance - 1],
                "освещенность растет с расстоянием: {distance}"
            );
        }
    }

    #[test]
    fn snapshot_reports_population_and_extremes() {
        let mut jar = Jar::new(20);
        jar.rng = Rng::new(0xC0FFEE);
        spawn(&mut jar, genome_top(), &[(9, 2), (10, 2), (11, 2)]);

        let snapshot = jar.snapshot();
        assert_eq!(snapshot.population, 3);
        assert_eq!(snapshot.max_energy, 0);
        assert!(snapshot.max_minerals >= 100);

        // Клетки выполняют действия: в снимке появляется последнее действие.
        for _ in 0..4 {
            jar.simulation_step();
        }
        let snapshot = jar.snapshot();
        assert!(
            snapshot.actions.iter().any(Option::is_some),
            "после тиков не записано ни одного действия"
        );
    }

    #[test]
    fn field_fits_only_when_the_grid_fits_in_both_axes() {
        assert!(field_fits(8, 100, 800, 800), "поле ровно в окне");
        assert!(!field_fits(8, 100, 799, 800), "поле шире окна");
        assert!(!field_fits(8, 100, 800, 799), "поле выше окна");
        assert!(!field_fits(9, 100, 800, 800), "поле не влезает");
    }

    #[test]
    fn selection_exists_only_when_the_field_does_not_fit() {
        let mut app = App::new(Jar::new(20));
        app.window_size = (800, 800);

        app.cell_size = 8;
        assert!(
            !app.selection_active(),
            "поле помещается, выделения быть не должно"
        );

        app.cell_size = 64;
        assert!(
            app.selection_active(),
            "поле не помещается, выделение нужно"
        );
    }

    #[test]
    fn selection_moves_by_exactly_one_cell() {
        let mut app = App::new(Jar::new(20));
        app.window_size = (800, 800);
        app.cell_size = 64;
        app.cursor_x = 10;
        app.cursor_y = 10;

        app.move_selection(Direction::Right);
        assert_eq!((app.cursor_x, app.cursor_y), (11, 10));

        app.move_selection(Direction::Up);
        assert_eq!((app.cursor_x, app.cursor_y), (11, 9));
    }

    #[test]
    fn selection_does_not_move_when_the_field_fits() {
        let mut app = App::new(Jar::new(20));
        app.window_size = (800, 800);
        app.cell_size = 8;
        app.cursor_x = 10;
        app.cursor_y = 10;

        app.move_selection(Direction::Right);
        assert_eq!((app.cursor_x, app.cursor_y), (10, 10));
    }

    #[test]
    fn selection_stops_at_the_field_border() {
        let mut app = App::new(Jar::new(20));
        app.window_size = (800, 800);
        app.cell_size = 64;

        app.cursor_x = 0;
        app.cursor_y = 0;
        app.move_selection(Direction::Left);
        app.move_selection(Direction::Up);
        assert_eq!((app.cursor_x, app.cursor_y), (0, 0));

        app.cursor_x = 19;
        app.cursor_y = 19;
        app.move_selection(Direction::Right);
        app.move_selection(Direction::Down);
        assert_eq!((app.cursor_x, app.cursor_y), (19, 19));
    }

    #[test]
    fn zoom_in_creates_selection_in_the_center_of_the_view() {
        let mut app = App::new(Jar::new(20));
        app.window_size = (800, 800);
        app.cell_size = 8;
        app.fit_cell_size = 8;
        assert!(!app.selection_active());

        app.set_cell_size(64);

        assert!(app.selection_active());
        assert_eq!(
            (app.cursor_x, app.cursor_y),
            (10, 10),
            "выделение должно появиться в центре вида"
        );
    }

    #[test]
    fn repeat_interval_accelerates_and_stops_at_the_limit() {
        let start = repeat_interval(Duration::ZERO);
        assert_eq!(start, REPEAT_INTERVAL_START);

        let faster = repeat_interval(Duration::from_millis(500));
        assert!(faster < start, "интервал не ускорился");

        let fastest = repeat_interval(Duration::from_secs(10));
        assert_eq!(fastest, REPEAT_INTERVAL_MIN);
    }

    #[test]
    fn press_moves_once_and_repeat_takes_over_later() {
        let mut app = App::new(Jar::new(20));
        app.window_size = (800, 800);
        app.cell_size = 64;
        app.cursor_x = 10;

        let now = Instant::now();
        app.press_direction(Direction::Right);
        assert_eq!(app.cursor_x, 11, "нажатие должно сдвинуть на одну клетку");
        assert!(app.hold.is_some());

        // До истечения задержки автоповтора ничего не происходит.
        app.update_repeat(now);
        assert_eq!(app.cursor_x, 11);

        // После задержки выделение едет само.
        app.update_repeat(now + REPEAT_DELAY + Duration::from_millis(1));
        assert_eq!(app.cursor_x, 12);

        app.release_direction();
        assert!(app.hold.is_none());
    }

    #[test]
    fn direction_is_recognized_by_key() {
        assert_eq!(
            Direction::from_key(PhysicalKey::Code(KeyCode::KeyW)),
            Some(Direction::Up)
        );
        assert_eq!(
            Direction::from_key(PhysicalKey::Code(KeyCode::ArrowRight)),
            Some(Direction::Right)
        );
        assert_eq!(Direction::from_key(PhysicalKey::Code(KeyCode::Space)), None);
    }

    #[test]
    fn flash_marks_cells_that_changed() {
        let mut jar = Jar::new(20);
        jar.rng = Rng::new(0xC0FFEE);

        let mut app = App::new(jar);
        app.update_flashes();
        assert!(
            app.flashes.iter().all(|flash| *flash == 0),
            "вспышки появились без изменений"
        );

        // Появилась новая клетка — на её месте вспышка.
        let index = 9 + 2 * 20;
        app.jar.creatures[index] = Some(Creature {
            genome: genome_top(),
            energy: 0,
            minerals: 0,
            age: 0,
            last_action: None,
        });
        app.snapshot = app.jar.snapshot();
        app.update_flashes();

        assert!(app.flashes[index] > 0, "рождение клетки не вызвало вспышку");
        assert!(
            app.flashes[8 + 2 * 20] == 0,
            "вспышка появилась на пустой клетке"
        );
    }
}
