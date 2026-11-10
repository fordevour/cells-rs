use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
/// Максимальная скорость: столько тиков за кадр.
const MAX_SPEED: u32 = u32::MAX;
/// Максимальное замедление: один тик раз в столько кадров.
const MAX_FRAMES_PER_TICK: u32 = 64;

fn main() {
    let mut jar = Jar::new(100);
    let c0 = Creature {
        genome: CreatureGenome {
            line: vec![
                CreatureAction::PhotoGrow,
                CreatureAction::EatMinerals,
                CreatureAction::Split,
            ],
            line_pointer: 0,
        },
        energy: 0,
        minerals: 0,
        age: 0,
    };
    jar.creatures[5 + (5 * 10)] = Some(c0);

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
    /// Основной режим: существа и минералы.
    Creatures,
    /// Только концентрация минералов.
    Minerals,
    /// Только энергия существ.
    Energy,
}

impl ViewMode {
    /// Следующий режим по кругу.
    fn next(self) -> Self {
        match self {
            Self::Creatures => Self::Minerals,
            Self::Minerals => Self::Energy,
            Self::Energy => Self::Creatures,
        }
    }

    /// Подпись режима для заголовка окна.
    fn label(self) -> &'static str {
        match self {
            Self::Creatures => "creatures",
            Self::Minerals => "minerals",
            Self::Energy => "energy",
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
                        mineral_color(snapshot.minerals[index])
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
            };

            let px = offset_x + (x as u32 * cell) as i64;
            let py = offset_y + (y as u32 * cell) as i64;
            fill_rect(buffer, width, height, px, py, cell, cell, color);
        }
    }

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

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
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
                                let (new_0, new_1) = creature.split();
                                let j = nears[self.tick % nears.len()]; // Выбор места дочерней клетки
                                self.creatures[i] = Some(new_0); // На месте материнской клетки
                                self.creatures[j] = Some(new_1);
                            }
                        } else {
                            self.creatures[i] = Some(creature);
                        }
                    }
                    CreatureAction::PhotoGrow => {
                        creature.energy += 1;
                        self.creatures[i] = Some(creature);
                    }
                    CreatureAction::EatMinerals => {
                        if self.minerals[i] > 0 {
                            creature.minerals += 1;
                            self.minerals[i] -= 1;
                        }
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
    pub fn split(&self) -> (Self, Self) {
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

        (c_0, c_1)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CreatureAction {
    Split,
    PhotoGrow,
    EatMinerals,
}
#[derive(Clone, Debug)]
pub struct CreatureGenome {
    /// Нить "ДНК"
    pub line: Vec<CreatureAction>,
    pub line_pointer: usize,
}
impl CreatureGenome {
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
}
