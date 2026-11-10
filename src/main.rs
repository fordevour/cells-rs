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

const BACKGROUND_COLOR: u32 = 0x00FF_FFFF;
const BORDER_COLOR: u32 = 0x0000_0000;
const CREATURE_COLOR: u32 = 0x0040_d060;
/// Толщина внешней рамки поля в пикселях.
const FIELD_BORDER_THICKNESS: u32 = 2;
/// Толщина рамок обычных клеток.
const CELL_BORDER_THICKNESS: u32 = 1;
/// При этом размере клетки курсор двигается на одну клетку за нажатие.
const CURSOR_REFERENCE_CELL_SIZE: u32 = 32;
/// Максимальный шаг курсора в клетках при сильном отдалении.
const MAX_CURSOR_STEP: u32 = 8;
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
fn seeded_jar() -> Jar {
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
fn genome_top() -> CreatureGenome {
    CreatureGenome::standard()
}

/// Прыжки по ДНК вместо части делений: правый край.
fn genome_right() -> CreatureGenome {
    let mut genome = CreatureGenome::standard();
    genome.line[3] = CreatureAction::Jump(1);
    genome.line[7] = CreatureAction::Jump(2);
    genome.line[11] = CreatureAction::Jump(1);
    genome
}

/// Больше поедания минералов: нижний край.
fn genome_bottom() -> CreatureGenome {
    let mut genome = CreatureGenome::standard();
    genome.line[4] = CreatureAction::EatMinerals;
    genome.line[8] = CreatureAction::EatMinerals;
    genome
}

/// Больше делений: левый край.
fn genome_left() -> CreatureGenome {
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
enum ViewMode {
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
}

impl ViewMode {
    /// Следующий режим по кругу.
    fn next(self) -> Self {
        match self {
            Self::Creatures => Self::Minerals,
            Self::Minerals => Self::Energy,
            Self::Energy => Self::Light,
            Self::Light => Self::Genome,
            Self::Genome => Self::Creatures,
        }
    }

    /// Подпись режима для заголовка окна.
    fn label(self) -> &'static str {
        match self {
            Self::Creatures => "creatures",
            Self::Minerals => "minerals",
            Self::Energy => "energy",
            Self::Light => "light",
            Self::Genome => "genome",
        }
    }
}

/// Приложение winit: хранит Jar, раз в кадр обновляет снапшот и рисует его.
struct App {
    jar: Jar,
    snapshot: JarSnapshot,

    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,

    /// Размер одной клетки в пикселях.
    cell_size: u32,
    /// Позиция выбранной клетки.
    cursor_x: usize,
    cursor_y: usize,
    /// Скорость симуляции.
    speed: Speed,
    /// Сколько кадров накоплено при замедленном режиме.
    frame_accum: u32,
    /// Приостановлена ли симуляция.
    paused: bool,
    /// Режим просмотра поля.
    view_mode: ViewMode,
    next_frame: Instant,
}

impl App {
    fn new(jar: Jar) -> Self {
        let snapshot = jar.snapshot();
        let cursor_x = jar.size / 2;
        let cursor_y = jar.size / 2;
        Self {
            jar,
            snapshot,
            window: None,
            context: None,
            surface: None,
            cell_size: 16,
            cursor_x,
            cursor_y,
            speed: Speed::TicksPerFrame(1),
            frame_accum: 0,
            paused: false,
            view_mode: ViewMode::Creatures,
            next_frame: Instant::now(),
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Обновляет заголовок окна: тик, масштаб, скорость, режим и состояние паузы.
    fn update_title(&self) {
        let Some(window) = &self.window else {
            return;
        };

        let pause = if self.paused { ", пауза" } else { "" };
        window.set_title(&format!(
            "Cells — тик {}, масштаб {}, скорость {}, режим {}{}",
            self.snapshot.tick,
            self.cell_size,
            self.speed.label(),
            self.view_mode.label(),
            pause
        ));
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

        if new_size != self.cell_size {
            self.cell_size = new_size;
            self.request_redraw();
        }
    }

    /// Двигает курсор. Чем сильнее отдаление, тем больше шаг за одно нажатие.
    fn move_cursor(&mut self, dx: isize, dy: isize) {
        let step = (CURSOR_REFERENCE_CELL_SIZE / self.cell_size).clamp(1, MAX_CURSOR_STEP) as isize;
        let last = self.snapshot.size.saturating_sub(1) as isize;

        self.cursor_x = (self.cursor_x as isize + dx * step).clamp(0, last) as usize;
        self.cursor_y = (self.cursor_y as isize + dy * step).clamp(0, last) as usize;
        self.request_redraw();
    }

    fn render(&mut self) {
        let (Some(window), Some(surface)) = (self.window.as_ref(), self.surface.as_mut()) else {
            return;
        };

        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };

        if let Err(err) = surface.resize(width, height) {
            eprintln!("Не удалось изменить размер поверхности: {err}");
            return;
        }

        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };

        let width = width.get();
        let height = height.get();
        buffer.fill(BACKGROUND_COLOR);
        draw_snapshot(
            &mut buffer,
            width,
            height,
            &self.snapshot,
            self.view_mode,
            self.cell_size,
            self.cursor_x,
            self.cursor_y,
        );

        if let Err(err) = buffer.present() {
            eprintln!("Не удалось показать кадр: {err}");
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
            .with_inner_size(LogicalSize::new(800.0, 800.0));

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
        self.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.surface = None;
        self.context = None;
        self.window = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.request_redraw(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyW | KeyCode::ArrowUp) => {
                            self.move_cursor(0, -1)
                        }
                        PhysicalKey::Code(KeyCode::KeyS | KeyCode::ArrowDown) => {
                            self.move_cursor(0, 1)
                        }
                        PhysicalKey::Code(KeyCode::KeyA | KeyCode::ArrowLeft) => {
                            self.move_cursor(-1, 0)
                        }
                        PhysicalKey::Code(KeyCode::KeyD | KeyCode::ArrowRight) => {
                            self.move_cursor(1, 0)
                        }
                        PhysicalKey::Code(KeyCode::Space) => self.toggle_pause(),
                        PhysicalKey::Code(KeyCode::Tab) => self.toggle_view_mode(),
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
        if self.paused {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        let now = Instant::now();

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
                self.request_redraw();
            }
            self.next_frame = now + FRAME_TIME;
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

fn draw_snapshot(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    snapshot: &JarSnapshot,
    view_mode: ViewMode,
    cell_size: u32,
    cursor_x: usize,
    cursor_y: usize,
) {
    let cell = cell_size.max(1);
    let grid = snapshot.size as u32 * cell;

    // Курсор всегда находится в центре окна, поэтому смещение карты
    // рассчитывается относительно выбранной клетки.
    let cursor_center_x = (cursor_x as u32 * cell + cell / 2) as i64;
    let cursor_center_y = (cursor_y as u32 * cell + cell / 2) as i64;
    let offset_x = width as i64 / 2 - cursor_center_x;
    let offset_y = height as i64 / 2 - cursor_center_y;

    // Заливка клеток: существа, минералы или пустое пространство.
    for y in 0..snapshot.size {
        for x in 0..snapshot.size {
            let index = x + y * snapshot.size;
            let color = match view_mode {
                ViewMode::Creatures => {
                    if snapshot.creatures[index] {
                        CREATURE_COLOR
                    } else {
                        BACKGROUND_COLOR
                    }
                }
                ViewMode::Minerals => mineral_color(snapshot.minerals[index]),
                ViewMode::Energy => {
                    if snapshot.creatures[index] {
                        energy_color(snapshot.energies[index])
                    } else {
                        BACKGROUND_COLOR
                    }
                }
                // Свет рисуется отдельным пиксельным градиентом ниже.
                ViewMode::Light => continue,
                ViewMode::Genome => snapshot.genome_colors[index].unwrap_or(BACKGROUND_COLOR),
            };

            let px = offset_x + (x as u32 * cell) as i64;
            let py = offset_y + (y as u32 * cell) as i64;
            fill_rect(buffer, width, height, px, py, cell, cell, color);
        }
    }

    if view_mode == ViewMode::Light {
        draw_light_field(
            buffer,
            width,
            height,
            (offset_x, offset_y),
            grid,
            cell,
            snapshot.size,
        );
    } else {
        // Тонкая черная рамка вокруг каждой клетки поля.
        for i in 0..=snapshot.size {
            let x = offset_x + (i as u32 * cell) as i64;
            let line_x = if i == snapshot.size { x - 1 } else { x };
            fill_rect(
                buffer,
                width,
                height,
                line_x,
                offset_y,
                CELL_BORDER_THICKNESS,
                grid,
                BORDER_COLOR,
            );

            let y = offset_y + (i as u32 * cell) as i64;
            let line_y = if i == snapshot.size { y - 1 } else { y };
            fill_rect(
                buffer,
                width,
                height,
                offset_x,
                line_y,
                grid,
                CELL_BORDER_THICKNESS,
                BORDER_COLOR,
            );
        }
    }

    // Более толстая рамка по внешним границам Jar.
    let border = FIELD_BORDER_THICKNESS.min(cell);
    fill_rect(
        buffer,
        width,
        height,
        offset_x,
        offset_y,
        grid,
        border,
        BORDER_COLOR,
    );
    fill_rect(
        buffer,
        width,
        height,
        offset_x,
        offset_y + grid as i64 - border as i64,
        grid,
        border,
        BORDER_COLOR,
    );
    fill_rect(
        buffer,
        width,
        height,
        offset_x,
        offset_y,
        border,
        grid,
        BORDER_COLOR,
    );
    fill_rect(
        buffer,
        width,
        height,
        offset_x + grid as i64 - border as i64,
        offset_y,
        border,
        grid,
        BORDER_COLOR,
    );

    // Выделенная клетка: более толстая рамка.
    let selection_border = (cell / 5).clamp(2, 4).min(cell);
    let cursor_px = offset_x + (cursor_x as u32 * cell) as i64;
    let cursor_py = offset_y + (cursor_y as u32 * cell) as i64;
    draw_cell_frame(
        buffer,
        width,
        height,
        cursor_px,
        cursor_py,
        cell,
        selection_border,
        BORDER_COLOR,
    );
}

/// Плавный пиксельный градиент освещенности: свет у краев, тьма в центре.
fn draw_light_field(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    origin: (i64, i64),
    grid: u32,
    cell: u32,
    size: usize,
) {
    let (offset_x, offset_y) = origin;
    let reach_px = (light_reach(size) as u32 * cell).max(1);

    // Палитра по расстоянию до ближайшего края в пикселях.
    let mut palette = Vec::with_capacity(reach_px as usize);
    for distance in 0..reach_px {
        let remaining = reach_px - distance;
        let intensity = (remaining * 255).div_ceil(reach_px) as u8;
        palette.push(light_color(intensity));
    }
    let dark = light_color(0);

    let x0 = offset_x.max(0).min(width as i64);
    let y0 = offset_y.max(0).min(height as i64);
    let x1 = (offset_x + grid as i64).max(0).min(width as i64);
    let y1 = (offset_y + grid as i64).max(0).min(height as i64);

    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let grid_size = grid as i64;
    for py in y0..y1 {
        let local_y = py - offset_y;
        let dy = local_y.min(grid_size - 1 - local_y) as usize;
        let row = (py as u32 * width) as usize;
        for px in x0..x1 {
            let local_x = px - offset_x;
            let dx = local_x.min(grid_size - 1 - local_x) as usize;
            let distance = dx.min(dy);
            let color = palette.get(distance).copied().unwrap_or(dark);
            buffer[row + px as usize] = color;
        }
    }
}

fn draw_cell_frame(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    x: i64,
    y: i64,
    cell: u32,
    thickness: u32,
    color: u32,
) {
    let thickness = thickness.clamp(1, cell);

    fill_rect(buffer, width, height, x, y, cell, thickness, color);
    fill_rect(
        buffer,
        width,
        height,
        x,
        y + cell as i64 - thickness as i64,
        cell,
        thickness,
        color,
    );
    fill_rect(buffer, width, height, x, y, thickness, cell, color);
    fill_rect(
        buffer,
        width,
        height,
        x + cell as i64 - thickness as i64,
        y,
        thickness,
        cell,
        color,
    );
}

fn fill_rect(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    x: i64,
    y: i64,
    rect_width: u32,
    rect_height: u32,
    color: u32,
) {
    let x0 = x.max(0).min(width as i64) as u32;
    let y0 = y.max(0).min(height as i64) as u32;
    let x1 = (x + rect_width as i64).max(0).min(width as i64) as u32;
    let y1 = (y + rect_height as i64).max(0).min(height as i64) as u32;

    if x0 >= x1 || y0 >= y1 {
        return;
    }

    for row in y0..y1 {
        let start = (row * width + x0) as usize;
        let end = (row * width + x1) as usize;
        buffer[start..end].fill(color);
    }
}

/// Белый фон с добавлением синего: чем больше минералов, тем заметнее оттенок.
fn mineral_color(minerals: usize) -> u32 {
    let amount = minerals.min(100) as u32;
    let r = 255 - amount * 64 / 100;
    let g = 255 - amount * 43 / 100;
    let b = 255;
    rgb(r as u8, g as u8, b as u8)
}

/// Белый фон с добавлением оранжевого: чем больше энергии, тем заметнее оттенок.
fn energy_color(energy: usize) -> u32 {
    let amount = energy.min(100) as u32;
    let r = 255;
    let g = 255 - amount * 140 / 100;
    let b = 255 - amount * 230 / 100;
    rgb(r as u8, g as u8, b as u8)
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
fn genome_color(genome: &CreatureGenome) -> u32 {
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

                match creature.genome.get_action() {
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
        };
        let mut c_1 = Creature {
            genome: self.genome.clone(),
            energy: e_1,
            minerals: m_1,
            age: 0,
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
}
