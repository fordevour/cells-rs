//! Cells: жизнь в банке.
//!
//! Файл один: так проще видеть, как правила симуляции, отрисовка и ввод
//! связаны между собой. Внутри он разбит на разделы:
//!
//! * правила — [`Config`], [`Grid`], [`Jar`], [`Creature`];
//! * гены — [`ActionKind`], [`CreatureGenome`];
//! * интерфейс — [`Camera`], [`KeyRepeat`], [`SimClock`], [`Command`];
//! * картинка — [`ViewMode`], [`Canvas`].

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

/// Шкала зума: размеры клетки, между которыми переключается камера.
///
/// Ступени идут от мелких клеток к крупным. Благодаря общей шкале
/// зум всегда обратим: шаг вверх и шаг вниз возвращают тот же размер.
const ZOOM_LEVELS: [u32; 14] = [3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64];

/// Задержка перед автоповтором перемещения выделения.
const REPEAT_DELAY: Duration = Duration::from_millis(180);
/// Интервал автоповтора в начале удержания клавиши.
const REPEAT_INTERVAL_START: Duration = Duration::from_millis(45);
/// Минимальный интервал автоповтора: предельная скорость перемещения.
const REPEAT_INTERVAL_MIN: Duration = Duration::from_millis(12);
/// Нижняя граница срока жизни клетки в тиках.
const CREATURE_LIFETIME_MIN_TICKS: usize = 900;
/// Верхняя граница срока жизни клетки в тиках.
const CREATURE_LIFETIME_MAX_TICKS: usize = 1100;
/// Срок жизни по умолчанию: используется, когда клетку создают напрямую.
const CREATURE_LIFETIME_TICKS: usize = 1000;
/// Максимальный прирост энергии от фотосинтеза за тик (у самого края).
const MAX_PHOTOSYNTHESIS_ENERGY: usize = 4;
/// Сколько энергии тратит поедание одного минерала.
const EAT_MINERALS_ENERGY_COST: usize = 1;
/// Шанс мутации генома при делении, в процентах (для каждой дочерней клетки).
const MUTATION_CHANCE_PERCENT: u64 = 1;
/// Максимальная скорость: столько тиков за кадр.
const MAX_SPEED: u32 = u32::MAX;
/// Максимальное замедление: один тик раз в столько кадров.
const MAX_FRAMES_PER_TICK: u32 = 64;
/// Сколько генов в стандартном геноме.
const GENOME_LEN: usize = 16;
/// Сколько энергии нужно клетке для деления.
const SPLIT_ENERGY: usize = 100;
/// Сколько минералов нужно клетке для деления.
const SPLIT_MINERALS: usize = 10;
/// Сколько процентов энергии матери достается каждому ребенку.
const SPLIT_ENERGY_SHARE_PERCENT: usize = 40;
/// Сколько минералов изначально лежит в каждой клетке поля.
const INITIAL_MINERALS: usize = 100;
/// Сколько энергии клетка отдает случайному соседу за один ген Share.
const SHARE_AMOUNT: usize = 8;
/// Сколько энергии стоит одна атака.
const ATTACK_COST: usize = 2;
/// Сколько энергии снимает одна атака с жертвы.
///
/// Урон больше цены: нападающий тратит меньше, чем отнимает, но война
/// все равно идет на истощение - у каждой стороны свой фотосинтез.
const ATTACK_DAMAGE: usize = 4;
/// Порог энергии для условия LowEnergy гена JumpIf.
const LOW_ENERGY: usize = 30;
/// Минимальная схожесть состава ДНК, при которой сосед считается своим.
///
/// Мутировавший ребенок отдален от матери на один ген (~94%), поэтому
/// он остается своим; кластеры с разных краев банки различаются сильно
/// (~60-70%) и считаются чужими - на их стыке начинается война.
const KIN_SIMILARITY_PERCENT: u32 = 80;

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
/// Размер стороны мини-карты в пикселях.
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
/// Сколько миллисекунд на кадр разрешено тратить на тики симуляции.
///
/// Ограничение спасает окно от зависания, когда скорость выкручена вверх.
const TICK_BUDGET: Duration = Duration::from_millis(8);

// ===========================================================================
// Палитра
// ===========================================================================

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
    /// Цвета типов действий: по [`ActionKind::index`].
    action_colors: [u32; ActionKind::COUNT],
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
                0x00F2_B33D, // Share
                0x00D6_2B2B, // Attack
                0x000E_A5A5, // JumpIf
            ],
        }
    }
}

/// Опорная точка градиента: значение и цвет.
type Stop = (u32, (u32, u32, u32));

/// Рампа минералов: холодная, от светлого к тёмному.
const MINERAL_RAMP: [Stop; 5] = [
    (0, (207, 227, 245)),
    (64, (159, 201, 236)),
    (128, (95, 168, 222)),
    (192, (44, 116, 196)),
    (255, (18, 63, 126)),
];

/// Рампа энергии: тёплая, от светлого к тёмному.
const ENERGY_RAMP: [Stop; 5] = [
    (0, (255, 224, 184)),
    (64, (255, 192, 120)),
    (128, (247, 148, 72)),
    (192, (222, 90, 30)),
    (255, (122, 46, 8)),
];

/// Рампа освещенности: от почти черной тьмы в центре через синий
/// и теплый оранжевый к мягкому солнечному свету у края.
const LIGHT_RAMP: [Stop; 4] = [
    (0, (8, 10, 24)),
    (90, (36, 50, 112)),
    (180, (212, 116, 52)),
    (255, (255, 244, 190)),
];

// ===========================================================================
// Типы генов
// ===========================================================================

/// Тип гена: что именно делает клетка, выполняя этот ген.
///
/// Реестр типов — единственное место, где перечислены все гены.
/// Добавление нового гена требует добавить его сюда, а остальные
/// места (палитра, подписи, доли генома) соберутся компилятором.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionKind {
    /// Фотосинтез: клетка получает энергию из света.
    PhotoGrow,
    /// Еда: клетка тратит энергию и забирает минерал из поля.
    EatMinerals,
    /// Деление: клетка делится на две, если хватает энергии и минералов.
    Split,
    /// Прыжок: указатель ДНК сдвигается на дополнительное число.
    Jump,
    /// Щедрость: клетка отдает энергию случайному соседу.
    Share,
    /// Атака: клетка снимает энергию с соседа, а при нуле - убивает его.
    Attack,
    /// Условный прыжок: сдвиг по ДНК, только если выполнено условие.
    JumpIf,
}

impl ActionKind {
    /// Все типы генов. Порядок задает индексы в палитре и в составе генома.
    pub const ALL: [ActionKind; 7] = [
        Self::PhotoGrow,
        Self::EatMinerals,
        Self::Split,
        Self::Jump,
        Self::Share,
        Self::Attack,
        Self::JumpIf,
    ];

    /// Сколько всего типов генов.
    pub const COUNT: usize = Self::ALL.len();

    /// Индекс типа гена: по нему ищется цвет в палитре.
    pub const fn index(self) -> usize {
        match self {
            Self::PhotoGrow => 0,
            Self::EatMinerals => 1,
            Self::Split => 2,
            Self::Jump => 3,
            Self::Share => 4,
            Self::Attack => 5,
            Self::JumpIf => 6,
        }
    }

    /// Короткое имя типа гена.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PhotoGrow => "photo",
            Self::EatMinerals => "eat",
            Self::Split => "split",
            Self::Jump => "jump",
            Self::Share => "share",
            Self::Attack => "atk",
            Self::JumpIf => "jif",
        }
    }

    /// Оттенок гена в градусах HSV.
    pub const fn hue(self) -> f32 {
        match self {
            Self::PhotoGrow => 120.0,
            Self::EatMinerals => 240.0,
            Self::Split => 0.0,
            Self::Jump => 300.0,
            Self::Share => 45.0,
            Self::Attack => 350.0,
            Self::JumpIf => 180.0,
        }
    }
}

/// Условие гена [`CreatureAction::JumpIf`]: что должно быть верным,
/// чтобы указатель ДНК действительно сдвинулся.
///
/// Условие считается по соседям клетки, поэтому оно и порождает
/// разделение на "своих" и "чужих": клетка сама решает, делиться
/// ей или бить, глядя на геномы вокруг.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KinCondition {
    /// Рядом есть клетка с похожей ДНК.
    HasKin,
    /// Рядом нет клеток с похожей ДНК.
    NoKin,
    /// Рядом есть клетка с чужой ДНК.
    HasFoe,
    /// Рядом нет клеток с чужой ДНК.
    NoFoe,
    /// У самой клетки мало энергии.
    LowEnergy,
}

impl KinCondition {
    /// Все условия. Порядок задает индексы при мутации.
    pub const ALL: [KinCondition; 5] = [
        Self::HasKin,
        Self::NoKin,
        Self::HasFoe,
        Self::NoFoe,
        Self::LowEnergy,
    ];

    /// Сколько всего условий.
    pub const COUNT: usize = Self::ALL.len();

    /// Короткое имя условия.
    pub const fn label(self) -> &'static str {
        match self {
            Self::HasKin => "kin",
            Self::NoKin => "nokin",
            Self::HasFoe => "foe",
            Self::NoFoe => "nofoe",
            Self::LowEnergy => "low",
        }
    }

    /// Следующее условие по кругу: так мутация не оставляет условие прежним.
    pub const fn next(self) -> Self {
        match self {
            Self::HasKin => Self::NoKin,
            Self::NoKin => Self::HasFoe,
            Self::HasFoe => Self::NoFoe,
            Self::NoFoe => Self::LowEnergy,
            Self::LowEnergy => Self::HasKin,
        }
    }
}

/// Действие, которое клетка выполняет по очередному гену.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureAction {
    /// Деление.
    Split,
    /// Фотосинтез.
    PhotoGrow,
    /// Еда.
    EatMinerals,
    /// Сдвинуть указатель ДНК на X (вдобавок к обычному +1).
    Jump(isize),
    /// Отдать часть энергии случайному соседу.
    Share,
    /// Снять энергию с случайного соседа; при нуле - убить его.
    Attack,
    /// Сдвинуть указатель ДНК на X, только если выполнено условие.
    JumpIf(KinCondition, isize),
}

impl CreatureAction {
    /// Тип гена, к которому относится действие.
    pub fn kind(self) -> ActionKind {
        match self {
            CreatureAction::Split => ActionKind::Split,
            CreatureAction::PhotoGrow => ActionKind::PhotoGrow,
            CreatureAction::EatMinerals => ActionKind::EatMinerals,
            CreatureAction::Jump(_) => ActionKind::Jump,
            CreatureAction::Share => ActionKind::Share,
            CreatureAction::Attack => ActionKind::Attack,
            CreatureAction::JumpIf(_, _) => ActionKind::JumpIf,
        }
    }

    /// Дельта прыжка, если у действия есть параметр сдвига.
    ///
    /// Условный прыжок сдвигает указатель ровно так же, как обычный:
    /// разница только в том, срабатывает ли сдвиг вообще.
    pub fn jump_delta(self) -> Option<isize> {
        match self {
            CreatureAction::Jump(delta) => Some(delta),
            CreatureAction::JumpIf(_, delta) => Some(delta),
            _ => None,
        }
    }
}

/// Способ мутации генома.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MutationKind {
    /// Вставить новый ген.
    Insert,
    /// Изменить параметр гена: дельту прыжка или условие условного прыжка.
    Value,
    /// Удалить ген.
    Remove,
    /// Заменить ген на другой.
    Replace,
}

impl MutationKind {
    /// Все способы мутации.
    pub const ALL: [MutationKind; 4] = [Self::Insert, Self::Value, Self::Remove, Self::Replace];
}

// ===========================================================================
// Случайность
// ===========================================================================

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
pub fn random_action(rng: &mut Rng) -> CreatureAction {
    match ActionKind::ALL[rng.below(ActionKind::COUNT)] {
        ActionKind::PhotoGrow => CreatureAction::PhotoGrow,
        ActionKind::EatMinerals => CreatureAction::EatMinerals,
        ActionKind::Split => CreatureAction::Split,
        ActionKind::Jump => CreatureAction::Jump(random_jump_delta(rng)),
        ActionKind::Share => CreatureAction::Share,
        ActionKind::Attack => CreatureAction::Attack,
        ActionKind::JumpIf => CreatureAction::JumpIf(
            KinCondition::ALL[rng.below(KinCondition::COUNT)],
            random_jump_delta(rng),
        ),
    }
}

/// Случайное условие для гена JumpIf.
pub fn random_condition(rng: &mut Rng) -> KinCondition {
    KinCondition::ALL[rng.below(KinCondition::COUNT)]
}

/// Случайное ненулевое значение для гена Jump.
///
/// Диапазон значений: от -4 до 4, причем ноль не выпадает:
/// ген прыжка всегда что-то сдвигает.
pub fn random_jump_delta(rng: &mut Rng) -> isize {
    let delta = rng.below(8) as isize - 4;
    if delta >= 0 { delta + 1 } else { delta }
}

/// Новое значение дельты прыжка: текущее значение, сдвинутое на случайную
/// величину и зажатое в диапазон -4..=4 без нуля.
///
/// Если зажатие свело сдвиг к нулю или к прежнему значению, мутация
/// меняет знак: она обязана что-то менять.
fn new_jump_value(current: isize, rng: &mut Rng) -> isize {
    let mut next = current + random_jump_delta(rng);
    if next == 0 {
        // Нулевой прыжок бессмысленен: оставляем направление прежним.
        next = if current < 0 { -1 } else { 1 };
    }
    next = next.clamp(-4, 4);
    if next == current {
        // Мутация обязана что-то менять: если не вышло - меняем знак.
        next = -current;
    }
    next
}

// ===========================================================================
// Сетка и настройки
// ===========================================================================

/// Сетка поля: квадратная сетка клеток с индексацией построчно.
///
/// Индекс клетки — это `x + y * size`, поэтому все обращения к полю
/// считаются одним и тем же способом.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    /// Ширина и высота поля.
    pub size: usize,
}

impl Grid {
    /// Создает сетку такого размера.
    pub fn new(size: usize) -> Self {
        Self { size }
    }

    /// Сколько всего клеток в поле.
    pub fn len(self) -> usize {
        self.size * self.size
    }

    /// Пусто ли поле.
    pub fn is_empty(self) -> bool {
        self.size == 0
    }

    /// Индекс клетки по координатам.
    pub fn idx(self, x: usize, y: usize) -> usize {
        x + y * self.size
    }

    /// Координаты клетки по индексу.
    pub fn xy(self, index: usize) -> (usize, usize) {
        if self.size == 0 {
            return (0, 0);
        }
        (index % self.size, index / self.size)
    }

    /// Внутри ли поля клетка.
    pub fn in_bounds(self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size
    }

    /// Соседи клетки по четырем сторонам, без выхода за границы поля.
    ///
    /// Порядок: вправо, влево, вниз, вверх.
    pub fn neighbors4(self, index: usize) -> impl Iterator<Item = usize> {
        let (x, y) = self.xy(index);
        let directions = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)];

        directions.into_iter().filter_map(move |(dx, dy)| {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 {
                return None;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if !self.in_bounds(nx, ny) {
                return None;
            }
            Some(self.idx(nx, ny))
        })
    }
}

/// Настройки симуляции.
///
/// Один и тот же набор правил применяется ко всем банкам: меняя
/// [`Config`], можно получить короткоживущие клетки или быстрое
/// деление, не затрагивая код правил.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Нижняя граница случайного срока жизни клетки.
    pub lifetime_min_ticks: usize,
    /// Верхняя граница случайного срока жизни клетки.
    pub lifetime_max_ticks: usize,
    /// Максимальный прирост энергии от фотосинтеза за тик.
    pub max_photosynthesis_energy: usize,
    /// Сколько энергии тратит поедание одного минерала.
    pub eat_minerals_energy_cost: usize,
    /// Шанс мутации генома при делении, в процентах.
    pub mutation_chance_percent: u64,
    /// Сколько энергии нужно клетке для деления.
    pub split_energy: usize,
    /// Сколько минералов нужно клетке для деления.
    pub split_minerals: usize,
    /// Сколько процентов энергии матери достается каждому ребенку.
    pub split_energy_share_percent: usize,
    /// Сколько минералов изначально лежит в каждой клетке поля.
    pub initial_minerals: usize,
    /// Сколько генов в стандартном геноме.
    pub genome_len: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lifetime_min_ticks: CREATURE_LIFETIME_MIN_TICKS,
            lifetime_max_ticks: CREATURE_LIFETIME_MAX_TICKS,
            max_photosynthesis_energy: MAX_PHOTOSYNTHESIS_ENERGY,
            eat_minerals_energy_cost: EAT_MINERALS_ENERGY_COST,
            mutation_chance_percent: MUTATION_CHANCE_PERCENT,
            split_energy: SPLIT_ENERGY,
            split_minerals: SPLIT_MINERALS,
            split_energy_share_percent: SPLIT_ENERGY_SHARE_PERCENT,
            initial_minerals: INITIAL_MINERALS,
            genome_len: GENOME_LEN,
        }
    }
}

/// Случайный срок жизни клетки: от нижней до верхней границы из настроек.
///
/// Разброс нужен, чтобы целые поколения не умирали в один и тот же тик:
/// смерть волной иначе снова превращается в регулярные кольца.
fn random_lifespan(rng: &mut Rng, config: &Config) -> usize {
    let min = config.lifetime_min_ticks;
    let max = config.lifetime_max_ticks.max(min);
    min + rng.below(max - min + 1)
}

// ===========================================================================
// Геном
// ===========================================================================

/// Нить "ДНК" клетки.
#[derive(Clone, Debug)]
pub struct CreatureGenome {
    /// Гены клетки: порядок определяет, что клетка делает по кругу.
    line: Vec<CreatureAction>,
    /// Текущий указатель на ген.
    pub line_pointer: usize,
    /// Цвет генома: кешируется, потому что считается из 16 оттенков.
    color: u32,
    /// Доли генов каждого типа: кешируются, потому что считаются
    /// с сортировкой остатков, а нужны при каждом взаимодействии с соседом.
    mix: [u8; ActionKind::COUNT],
}

impl CreatureGenome {
    /// Стандартный геном из 16 генов.
    ///
    /// Базовый паттерн `[PhotoGrow, PhotoGrow, EatMinerals, Split]`
    /// повторяется четыре раза.
    pub fn standard() -> Self {
        Self::standard_with(GENOME_LEN)
    }

    /// Стандартный геном заданной длины.
    pub fn standard_with(len: usize) -> Self {
        let pattern = [
            CreatureAction::PhotoGrow,
            CreatureAction::PhotoGrow,
            CreatureAction::EatMinerals,
            CreatureAction::Split,
        ];

        let line = pattern.iter().cycle().take(len.max(1)).copied().collect();

        Self::from_line(line)
    }

    /// Создает геном из готовой нити ДНК.
    ///
    /// Пустой геном — ошибка: по нему нечего выполнять,
    /// поэтому конструктор отказывается его создавать.
    pub fn from_line(line: Vec<CreatureAction>) -> Self {
        assert!(!line.is_empty(), "геном не может быть пустым");

        let color = compute_genome_color(&line);
        let mix = compute_mix(&line);
        Self {
            line,
            line_pointer: 0,
            color,
            mix,
        }
    }

    /// Гены клетки.
    pub fn genes(&self) -> &[CreatureAction] {
        &self.line
    }

    /// Сколько генов в геноме.
    pub fn len(&self) -> usize {
        self.line.len()
    }

    /// Пуст ли геном: такого генома не бывает, но проверка handy.
    pub fn is_empty(&self) -> bool {
        self.line.is_empty()
    }

    /// Текущий указатель на ген.
    pub fn pointer(&self) -> usize {
        self.line_pointer
    }

    /// Ставит указатель на ген.
    pub fn set_pointer(&mut self, pointer: usize) {
        self.line_pointer = pointer;
    }

    /// Цвет генома: одинаковые ДНК окрашены одинаково.
    pub fn color(&self) -> u32 {
        self.color
    }

    /// Доли генов каждого типа: по ним считается сходство с соседями.
    pub fn mix(&self) -> [u8; ActionKind::COUNT] {
        self.mix
    }

    /// Сходство этого генома с другим: 100 - одинаковый состав ДНК.
    ///
    /// Мера не смотрит на порядок генов, поэтому вставка, удаление
    /// и перестановка не делают клетку чужой - только замены генов,
    /// накопленные поколениями, постепенно отдаляют потомка от родичей.
    pub fn similarity_to(&self, other: &CreatureGenome) -> u32 {
        kin_similarity(&self.mix, &other.mix)
    }

    /// Копия генома с измененными генами: `(позиция, ген)`.
    ///
    /// Удобно собирать варианты геномов, не лезя внутрь нити.
    pub fn with_genes(self, edits: &[(usize, CreatureAction)]) -> Self {
        let mut genome = self;
        for &(index, action) in edits {
            if index < genome.line.len() {
                genome.line[index] = action;
            }
        }
        genome.recompute_caches();
        genome
    }

    /// Сдвигает указатель ДНК на `delta` (не считая обычного шага +1).
    pub fn jump(&mut self, delta: isize) {
        self.line_pointer = apply_jump(self.line_pointer, delta);
    }

    /// Мутирует геном одним случайным способом.
    pub fn mutate(&mut self, rng: &mut Rng) {
        let kind = MutationKind::ALL[rng.below(MutationKind::ALL.len())];
        self.apply_mutation(kind, rng);
    }

    /// Мутирует геном выбранным способом.
    ///
    /// Так можно проверить каждый вид мутации по отдельности.
    pub fn apply_mutation(&mut self, kind: MutationKind, rng: &mut Rng) {
        match kind {
            MutationKind::Insert => {
                // Вставка нового гена в случайную позицию.
                let index = rng.below(self.line.len() + 1);
                self.line.insert(index, random_action(rng));
            }
            MutationKind::Value => {
                // Изменение параметра гена; если параметрических генов нет -
                // обычная замена гена.
                if !self.mutate_value(rng) {
                    self.replace_gene(rng);
                }
            }
            MutationKind::Remove => {
                // Удаление гена, но геном никогда не остается пустым.
                if self.line.len() > 1 {
                    let index = rng.below(self.line.len());
                    self.line.remove(index);
                } else {
                    self.replace_gene(rng);
                }
            }
            MutationKind::Replace => self.replace_gene(rng),
        }

        self.recompute_caches();
    }

    /// Заменяет случайный ген на случайный другой.
    fn replace_gene(&mut self, rng: &mut Rng) {
        let index = rng.below(self.line.len());
        self.line[index] = random_action(rng);
    }

    /// Изменяет параметр случайного параметрического гена.
    ///
    /// Параметрические гены - `Jump` и `JumpIf`: у них есть дельта сдвига,
    /// а у `JumpIf` еще и условие. Возвращает `false`, если такого гена нет.
    ///
    /// Новое значение дельты остается в диапазоне -4..=4 и никогда не равно
    /// нулю: ген прыжка всегда что-то сдвигает.
    fn mutate_value(&mut self, rng: &mut Rng) -> bool {
        let positions: Vec<usize> = self
            .line
            .iter()
            .enumerate()
            .filter_map(|(index, action)| action.jump_delta().map(|_| index))
            .collect();

        if positions.is_empty() {
            return false;
        }

        let index = positions[rng.below(positions.len())];
        match &mut self.line[index] {
            CreatureAction::Jump(value) => {
                *value = new_jump_value(*value, rng);
            }
            CreatureAction::JumpIf(condition, value) => {
                // Половина мутаций меняет сдвиг, половина - условие:
                // так геном учится иначе реагировать на соседей.
                if rng.below(2) == 0 {
                    *value = new_jump_value(*value, rng);
                } else {
                    // Мутация обязана что-то менять: условие сдвигается
                    // на следующее по кругу.
                    *condition = condition.next();
                }
            }
            _ => return false,
        }

        true
    }

    /// Пересчитывает кешированные цвет и состав генома.
    fn recompute_caches(&mut self) {
        self.color = compute_genome_color(&self.line);
        self.mix = compute_mix(&self.line);
    }

    /// Выдает текущее действие клетки, сдвигает указатель действия на следующий
    pub fn get_action(&mut self) -> CreatureAction {
        let now_pointer = self.line_pointer % self.line.len();
        self.line_pointer += 1;
        self.line[now_pointer]
    }
}

// ===========================================================================
// Клетка
// ===========================================================================

/// Клетка: геном, энергия, минералы и возраст.
#[derive(Clone, Debug)]
pub struct Creature {
    /// Геном клетки.
    pub genome: CreatureGenome,
    /// Энергия клетки.
    pub energy: usize,
    /// Минералы, которые клетка съела и унесла с собой.
    pub minerals: usize,
    /// Возраст клетки в тиках.
    pub age: usize,
    /// Возраст, в котором клетка умрет: задается случайно при рождении.
    pub lifespan: usize,
    /// Последнее выполненное действие: нужно для режима просмотра `action`.
    pub last_action: Option<CreatureAction>,
}

impl Creature {
    /// Новая клетка с указанным геномом.
    pub fn new(genome: CreatureGenome) -> Self {
        Self {
            genome,
            energy: 0,
            minerals: 0,
            age: 0,
            lifespan: CREATURE_LIFETIME_TICKS,
            last_action: None,
        }
    }

    /// Делит клетку на две.
    ///
    /// Каждая дочерняя клетка получает по 40% изначальной энергии, остаток теряется
    /// на само деление. Минералы делятся поровну без потерь: неделимый остаток
    /// достается материнской клетке. Обе дочерние клетки рождаются с нулевым возрастом.
    ///
    /// Каждая дочерняя клетка независимо может получить мутацию генома.
    pub fn split(&self, rng: &mut Rng, config: &Config) -> (Self, Self) {
        let energy = self.energy * config.split_energy_share_percent / 100;
        let minerals = self.minerals;

        let m_0 = (minerals / 2) + (minerals % 2);
        let m_1 = minerals - m_0;

        let mut c_0 = Creature::new(self.genome.clone());
        c_0.energy = energy;
        c_0.minerals = m_0;
        c_0.lifespan = random_lifespan(rng, config);

        let mut c_1 = Creature::new(self.genome.clone());
        c_1.energy = energy;
        c_1.minerals = m_1;
        c_1.lifespan = random_lifespan(rng, config);

        // Дети начинают ДНК с начала: указатель матери продолжает свой.
        c_0.genome.set_pointer(0);
        c_1.genome.set_pointer(0);

        if rng.chance_percent(config.mutation_chance_percent) {
            c_0.genome.mutate(rng);
        }
        if rng.chance_percent(config.mutation_chance_percent) {
            c_1.genome.mutate(rng);
        }

        (c_0, c_1)
    }
}

// ===========================================================================
// Банка
// ===========================================================================

/// Что случилось с клеткой за один тик.
enum Fate {
    /// Клетка остается на месте.
    Stay(Creature),
    /// Клетка разделилась: мать остается здесь, ребенок идет в `at`.
    Divide {
        /// Мать, остающаяся на своей клетке.
        mother: Creature,
        /// Ребенок, который появится в клетке `at`.
        child: Creature,
        /// Клетка, в которой появится ребенок.
        at: usize,
    },
}

/// Банка: квадратное поле с клетками, минералами и существами.
pub struct Jar {
    /// Ширина и высота пространства.
    pub size: usize,

    /// Номер тика, используется для рандома.
    pub tick: usize,

    /// Слой существ.
    pub creatures: Vec<Option<Creature>>,
    /// Слой минералов.
    pub minerals: Vec<usize>,
    /// Генератор случайности для мутаций и выбора направления деления.
    pub rng: Rng,
    /// Настройки правил симуляции.
    pub config: Config,

    /// Клетки, занятые детьми этого тика: переиспользуется между тиками.
    reserved: Vec<bool>,
    /// Дети, появившиеся в этом тике: переиспользуется между тиками.
    newborns: Vec<(usize, Creature)>,
}

impl Jar {
    /// Jar квадратная, со случайным зерном и стандартными настройками.
    pub fn new(size: usize) -> Self {
        Self::with_config(size, Config::default())
    }

    /// Jar с заданными настройками и случайным зерном.
    pub fn with_config(size: usize, config: Config) -> Self {
        Self::build(size, config, Rng::from_time())
    }

    /// Jar с заданным зерном: два таких Jar ведут себя одинаково.
    pub fn with_seed(size: usize, seed: u64) -> Self {
        Self::with_config_and_seed(size, Config::default(), seed)
    }

    /// Jar с заданными настройками и зерном.
    pub fn with_config_and_seed(size: usize, config: Config, seed: u64) -> Self {
        Self::build(size, config, Rng::new(seed))
    }

    /// Собирает банку нужного размера.
    fn build(size: usize, config: Config, rng: Rng) -> Self {
        let creatures = vec![None; size * size];
        let minerals = vec![config.initial_minerals; size * size];

        Self {
            size,
            tick: 0,
            creatures,
            minerals,
            rng,
            config,
            reserved: vec![false; size * size],
            newborns: Vec::new(),
        }
    }

    /// Случайная свободная клетка поля.
    ///
    /// Если случайные попытки не нашли пустую клетку (поле почти
    /// забито), берётся первая свободная.
    fn random_free_index(&mut self) -> usize {
        for _ in 0..100 {
            let index = self.rng.below(self.creatures.len());
            if self.creatures[index].is_none() {
                return index;
            }
        }

        (0..self.creatures.len())
            .find(|&index| self.creatures[index].is_none())
            .unwrap_or(0)
    }

    /// Создает снимок состояния мира для отрисовки.
    pub fn snapshot(&self) -> JarSnapshot {
        let mut snapshot = JarSnapshot::new(self.size);
        self.snapshot_into(&mut snapshot);
        snapshot
    }

    /// Заполняет снимок текущим состоянием мира, переиспользуя буферы.
    pub fn snapshot_into(&self, snapshot: &mut JarSnapshot) {
        let grid = Grid::new(self.size);

        snapshot.size = self.size;
        snapshot.tick = self.tick;
        snapshot.minerals.clear();
        snapshot.minerals.extend_from_slice(&self.minerals);
        snapshot.cells.clear();
        snapshot.cells.resize(grid.len(), CellView::EMPTY);

        let mut max_minerals = 0;
        let mut max_energy = 0;
        let mut population = 0;

        for (index, cell) in self.creatures.iter().enumerate() {
            max_minerals = max_minerals.max(self.minerals[index]);

            let Some(creature) = cell else {
                continue;
            };

            max_energy = max_energy.max(creature.energy);
            population += 1;

            snapshot.cells[index] = CellView {
                alive: true,
                energy: creature.energy,
                eaten: creature.minerals,
                age: creature.age,
                action: creature.last_action,
                genome_color: Some(creature.genome.color()),
                genome_mix: Some(genome_mix(&creature.genome)),
            };
        }

        snapshot.max_minerals = max_minerals;
        snapshot.max_energy = max_energy;
        snapshot.population = population;
    }

    /// Один тик симуляции: каждая клетка выполняет свой очередной ген.
    ///
    /// Дети, появившиеся при делении, попадают в поле после конца тика:
    /// поэтому результат не зависит от порядка обхода клеток.
    pub fn simulation_step(&mut self) {
        self.tick += 1;
        let grid = Grid::new(self.size);
        self.newborns.clear();

        for index in 0..grid.len() {
            let Some(mut creature) = self.creatures[index].take() else {
                continue;
            };

            creature.age += 1;

            if creature.age >= creature.lifespan {
                // Клетка прожила свой срок - умирает, а все ее минералы
                // возвращаются обратно на карту.
                self.minerals[index] += creature.minerals;
                continue;
            }

            let action = creature.genome.get_action();
            // Запоминаем, что именно сделала клетка: по этому видно цикл ДНК.
            creature.last_action = Some(action);

            match self.act(creature, action, index, &grid) {
                Fate::Stay(creature) => self.creatures[index] = Some(creature),
                Fate::Divide { mother, child, at } => {
                    self.creatures[index] = Some(mother);
                    self.reserved[at] = true;
                    self.newborns.push((at, child));
                }
            }
        }

        // Дети встают на место в конце тика: так они не выполняют действие
        // в тот же тик, в котором родились, и не занимают место друг у друга.
        for (at, child) in self.newborns.drain(..) {
            self.creatures[at] = Some(child);
            self.reserved[at] = false;
        }
    }

    /// Свободна ли клетка для ребенка.
    ///
    /// Правило не зависит от порядка обхода клеток: клетка свободна,
    /// если она пуста или если ее существо умирает в этом тике.
    fn cell_is_free(&self, neighbor: usize, index: usize) -> bool {
        if self.reserved[neighbor] {
            return false;
        }

        match self.creatures[neighbor].as_ref() {
            None => true,
            // Сосед еще не жил в этом тике: если он доживет до конца
            // тика, место занято.
            Some(creature) => neighbor > index && creature.age + 1 >= creature.lifespan,
        }
    }

    /// Выполняет действие клетки и решает, что с ней будет.
    fn act(
        &mut self,
        mut creature: Creature,
        action: CreatureAction,
        index: usize,
        grid: &Grid,
    ) -> Fate {
        match action {
            CreatureAction::PhotoGrow => {
                let (x, y) = grid.xy(index);
                creature.energy +=
                    light_level(x, y, self.size, self.config.max_photosynthesis_energy);
                Fate::Stay(creature)
            }
            CreatureAction::EatMinerals => {
                // Есть можно, только если хватает энергии и минералов.
                if self.minerals[index] > 0
                    && creature.energy >= self.config.eat_minerals_energy_cost
                {
                    creature.energy -= self.config.eat_minerals_energy_cost;
                    creature.minerals += 1;
                    self.minerals[index] -= 1;
                }
                Fate::Stay(creature)
            }
            CreatureAction::Split => {
                if creature.energy < self.config.split_energy
                    || creature.minerals < self.config.split_minerals
                {
                    return Fate::Stay(creature);
                }

                // Ищем свободного соседа: в него встанет ребенок.
                let mut free: Vec<usize> = Vec::new();
                for neighbor in grid.neighbors4(index) {
                    if self.cell_is_free(neighbor, index) {
                        free.push(neighbor);
                    }
                }

                if free.is_empty() {
                    // Делиться некуда: клетка остается на месте и спокойно
                    // выполняет остальные свои гены. Энергия не копится выше
                    // порога деления, иначе зажатая клетка копит ее бесконечно.
                    creature.energy = self.config.split_energy;
                    return Fate::Stay(creature);
                }

                let at = free[self.rng.below(free.len())];
                let (mother, child) = creature.split(&mut self.rng, &self.config);
                Fate::Divide { mother, child, at }
            }
            CreatureAction::Jump(delta) => {
                // get_action уже сдвинул указатель на +1, поэтому здесь
                // получается итоговый сдвиг +1+X.
                creature.genome.jump(delta);
                Fate::Stay(creature)
            }
            CreatureAction::Share => {
                // Делиться можно только с кем-то и только имея что.
                let Some(target) = self.random_neighbor(index, grid) else {
                    return Fate::Stay(creature);
                };

                let amount = creature.energy.min(SHARE_AMOUNT);
                if amount == 0 {
                    return Fate::Stay(creature);
                }

                creature.energy -= amount;
                if let Some(neighbour) = self.creatures[target].as_mut() {
                    neighbour.energy += amount;
                }
                Fate::Stay(creature)
            }
            CreatureAction::Attack => {
                // Бить можно только с кем-то и только имея чем платить.
                if creature.energy < ATTACK_COST {
                    return Fate::Stay(creature);
                }
                let Some(target) = self.random_neighbor(index, grid) else {
                    return Fate::Stay(creature);
                };

                creature.energy -= ATTACK_COST;
                self.strike(target, ATTACK_DAMAGE);
                Fate::Stay(creature)
            }
            CreatureAction::JumpIf(condition, delta) => {
                // Условие читает соседей: это единственный способ в ДНК
                // отличить своего от чужого и среагировать на голод.
                if self.condition_holds(condition, &creature, index, grid) {
                    creature.genome.jump(delta);
                }
                Fate::Stay(creature)
            }
        }
    }

    /// Случайная живая клетка среди соседей `index`; `None`, если соседей нет.
    ///
    /// Порядок обхода и номера клеток на выбор не влияют: всегда берется
    /// один из равных кандидатов.
    fn random_neighbor(&mut self, index: usize, grid: &Grid) -> Option<usize> {
        let alive: Vec<usize> = grid
            .neighbors4(index)
            .filter(|&neighbor| self.creatures[neighbor].is_some())
            .collect();

        if alive.is_empty() {
            return None;
        }

        Some(alive[self.rng.below(alive.len())])
    }

    /// Снимает с жертвы энергию; при нуле жертва погибает.
    ///
    /// Жертва может быть еще не обойдена этим тиком или уже обойдена -
    /// в обоих случаях она лежит в `creatures`, а основной цикл просто
    /// не найдет ее, если она выброшена. Новорожденных в `creatures`
    /// еще нет, поэтому конфликтов с `reserved` не бывает.
    fn strike(&mut self, target: usize, damage: usize) {
        let Some(mut victim) = self.creatures[target].take() else {
            return;
        };

        if victim.energy > damage {
            victim.energy -= damage;
            self.creatures[target] = Some(victim);
            return;
        }

        // Энергия кончилась: клетка умирает и возвращает минералы в поле.
        self.minerals[target] += victim.minerals;
    }

    /// Выполняет ли клетка условие `condition`, стоя на клетке `index`.
    ///
    /// Условия про соседей читаются целиком: `NoKin` и `NoFoe` - это
    /// отрицания `HasKin` и `HasFoe`, поэтому у одинокой клетки и то,
    /// и другое истинно.
    fn condition_holds(
        &self,
        condition: KinCondition,
        creature: &Creature,
        index: usize,
        grid: &Grid,
    ) -> bool {
        if condition == KinCondition::LowEnergy {
            return creature.energy < LOW_ENERGY;
        }

        let mine = creature.genome.mix();
        let (mut has_kin, mut has_foe) = (false, false);

        for neighbor in grid.neighbors4(index) {
            let Some(other) = self.creatures[neighbor].as_ref() else {
                continue;
            };

            if kin_similarity(&mine, &other.genome.mix()) >= KIN_SIMILARITY_PERCENT {
                has_kin = true;
            } else {
                has_foe = true;
            }
        }

        match condition {
            KinCondition::HasKin => has_kin,
            KinCondition::NoKin => !has_kin,
            KinCondition::HasFoe => has_foe,
            KinCondition::NoFoe => !has_foe,
            KinCondition::LowEnergy => false,
        }
    }

    /// Печатает поле в консоль: удобно для отладки симуляции.
    pub fn print(&self) {
        let grid = Grid::new(self.size);
        for y in 0..self.size {
            for x in 0..self.size {
                if self.creatures[grid.idx(x, y)].is_none() {
                    print!("·");
                } else {
                    print!("█");
                }
            }
            println!();
        }
    }
}

/// Данные одной клетки для отрисовки и панели инспектора.
#[derive(Clone, Copy, Debug)]
pub struct CellView {
    /// Есть ли в клетке существо.
    pub alive: bool,
    /// Энергия существа.
    pub energy: usize,
    /// Сколько минералов съело существо.
    pub eaten: usize,
    /// Возраст существа в тиках.
    pub age: usize,
    /// Последнее выполненное действие.
    pub action: Option<CreatureAction>,
    /// Цвет генома существа.
    pub genome_color: Option<u32>,
    /// Доли генов каждого типа в геноме.
    pub genome_mix: Option<[u8; ActionKind::COUNT]>,
}

impl CellView {
    /// Пустая клетка.
    pub const EMPTY: CellView = CellView {
        alive: false,
        energy: 0,
        eaten: 0,
        age: 0,
        action: None,
        genome_color: None,
        genome_mix: None,
    };
}

/// Снимок состояния мира: все клетки разом, в виде готовых к отрисовке данных.
#[derive(Clone, Debug)]
pub struct JarSnapshot {
    /// Ширина и высота снимка.
    pub size: usize,
    /// Номер тика, к которому относится снимок.
    pub tick: usize,
    /// Минералы в каждой клетке поля.
    pub minerals: Vec<usize>,
    /// Данные каждой клетки поля.
    pub cells: Vec<CellView>,
    /// Максимум минералов в поле: верхняя граница шкалы.
    pub max_minerals: usize,
    /// Максимум энергии в поле: верхняя граница шкалы.
    pub max_energy: usize,
    /// Сколько существ в поле.
    pub population: usize,
}

impl JarSnapshot {
    /// Пустой снимок на поле такого размера.
    pub fn new(size: usize) -> Self {
        Self {
            size,
            tick: 0,
            minerals: Vec::new(),
            cells: Vec::new(),
            max_minerals: 0,
            max_energy: 0,
            population: 0,
        }
    }

    /// Данные клетки по координатам.
    pub fn cell(&self, x: usize, y: usize) -> &CellView {
        let index = Grid::new(self.size).idx(x, y);
        self.cells.get(index).unwrap_or(&CellView::EMPTY)
    }
}

// ===========================================================================
// Состав генома и цвет
// ===========================================================================

/// Доли генов каждого типа в геноме.
///
/// Сумма долей всегда равна 255, поэтому полоска состава генома
/// заполняет клетку целиком.
pub fn genome_mix(genome: &CreatureGenome) -> [u8; ActionKind::COUNT] {
    genome.mix()
}

/// Считает доли генов каждого типа по нити ДНК.
fn compute_mix(line: &[CreatureAction]) -> [u8; ActionKind::COUNT] {
    let mut counts = [0u32; ActionKind::COUNT];
    for action in line {
        counts[action.kind().index()] += 1;
    }

    let total: u32 = counts.iter().sum();
    if total == 0 {
        return [0; ActionKind::COUNT];
    }

    // Доли считаем с округлением вниз, а остаток отдаем тем типам,
    // у которых дробная часть больше всего: так доля прыжков
    // не появляется из ниоткуда.
    let mut mix = [0u8; ActionKind::COUNT];
    let mut sum = 0u32;
    let mut remainders = [(0u32, 0usize); ActionKind::COUNT];

    for index in 0..ActionKind::COUNT {
        let exact = counts[index] * 255;
        let share = exact / total;
        mix[index] = share as u8;
        sum += share;
        remainders[index] = (exact % total, index);
    }

    let mut left = 255 - sum;
    remainders.sort_by_key(|(remainder, _)| std::cmp::Reverse(*remainder));

    for &(_, index) in &remainders {
        if left == 0 {
            break;
        }
        if mix[index] < 255 {
            mix[index] += 1;
            left -= 1;
        }
    }

    mix
}

/// Сходство двух составов ДНК: 100 - полностью одинаковые, 0 - никакой
/// общей структуры.
///
/// Сравниваются доли типов генов, поэтому порядок и позиции не важны:
/// вставка, удаление и перестановка гена не меняют сходство, а каждая
/// замена гена отдаляет клетку от родичей примерно на 6 процентов.
pub fn kin_similarity(left: &[u8; ActionKind::COUNT], right: &[u8; ActionKind::COUNT]) -> u32 {
    // Максимум разницы - 510: весь один геном в одном типе генов,
    // весь другой - в другом (255 + 255).
    let difference: u32 = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a.abs_diff(*b) as u32)
        .sum();

    100 - difference * 100 / 510
}

/// Цвет генома: взвешенная циркулярная средняя оттенков генов.
///
/// Насыщенность всегда 100%, поэтому смесь любых генов остается яркой
/// и никогда не превращается в серый цвет.
pub fn genome_color(genome: &CreatureGenome) -> u32 {
    genome.color()
}

/// Считает цвет генома по нити генов.
fn compute_genome_color(line: &[CreatureAction]) -> u32 {
    let Some(first) = line.first() else {
        return 0x00FF_FFFF;
    };

    let mut sin_sum = 0.0f32;
    let mut cos_sum = 0.0f32;

    for (i, action) in line.iter().enumerate() {
        let weight = (i + 1) as f32;
        let radians = action.kind().hue().to_radians();
        sin_sum += weight * radians.sin();
        cos_sum += weight * radians.cos();
    }

    // Если противоположные оттенки почти погасили друг друга,
    // берем оттенок первого гена. Насыщенность все равно останется 100%.
    let hue = if sin_sum * sin_sum + cos_sum * cos_sum < 1e-6 {
        first.kind().hue()
    } else {
        sin_sum.atan2(cos_sum).to_degrees().rem_euclid(360.0)
    };

    hsv_to_rgb(hue, 1.0, 0.85)
}

/// Переводит HSV в packed RGB.
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
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

/// Собирает packed RGB из трех каналов.
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

// ===========================================================================
// Свет
// ===========================================================================

/// Расстояние от клетки до ближайшего края Jar.
pub fn distance_to_edge(x: usize, y: usize, size: usize) -> usize {
    let last = size.saturating_sub(1);
    x.min(y)
        .min(last.saturating_sub(x))
        .min(last.saturating_sub(y))
}

/// Половина расстояния от края до центра — дальше света нет.
pub fn light_reach(size: usize) -> usize {
    (size / 4).max(1)
}

/// Сколько энергии дает фотосинтез в клетке `(x, y)`.
///
/// Свет идет от краев Jar к центру и полностью затухает на половине пути
/// к центру. Чем ближе клетка к краю, тем больше энергии она получает.
pub fn light_level(x: usize, y: usize, size: usize, max_energy: usize) -> usize {
    let reach = light_reach(size);
    let distance = distance_to_edge(x, y, size);

    if distance >= reach {
        return 0;
    }

    max_energy - max_energy * distance / reach
}

/// Прибавляет `delta` к указателю ДНК, не позволяя ему стать отрицательным.
pub fn apply_jump(pointer: usize, delta: isize) -> usize {
    let next = pointer as isize + delta;
    if next < 0 { 0 } else { next as usize }
}

// ===========================================================================
// Стартовые геномы
// ===========================================================================

/// Базовый геном: восемь генов, повторённых дважды.
///
/// Он одинаков у всех четырёх колоний: никакого готового поведения
/// в нём не зашито. Клетка умеет делиться энергией (`Share`), бить
/// соседа (`Attack`) и сдвигать указатель по ДНК, если голодна
/// (`JumpIf` с условием `LowEnergy`).
///
/// Всё остальное - условия гена `JumpIf`, порядок генов, их количество -
/// появляется само, в ходе мутаций. Именно поэтому у соседей
/// появляется привычка бить только чужих, а делиться - только своими.
pub(crate) fn genome_base() -> CreatureGenome {
    CreatureGenome::from_line(vec![
        CreatureAction::PhotoGrow,
        CreatureAction::PhotoGrow,
        CreatureAction::EatMinerals,
        CreatureAction::Split,
        CreatureAction::Share,
        CreatureAction::JumpIf(KinCondition::LowEnergy, 1),
        CreatureAction::Attack,
        CreatureAction::PhotoGrow,
        CreatureAction::PhotoGrow,
        CreatureAction::PhotoGrow,
        CreatureAction::EatMinerals,
        CreatureAction::Split,
        CreatureAction::Share,
        CreatureAction::JumpIf(KinCondition::LowEnergy, 1),
        CreatureAction::Attack,
        CreatureAction::PhotoGrow,
    ])
}

/// Верхний край: щедрый.
///
/// Отличий от базы одно: ещё один ген `Share`. Дальше всё решает
/// эволюция - либо щедрость закрепится, либо мутации её съедают.
pub(crate) fn genome_top() -> CreatureGenome {
    genome_base().with_genes(&[(8, CreatureAction::Share)])
}

/// Прыжки по ДНК: правый край.
///
/// Отличий от базы одно: ген `Jump`. Он сдвигает указатель и тем самым
/// меняет то, какие гены клетка исполняет по кругу.
pub(crate) fn genome_right() -> CreatureGenome {
    genome_base().with_genes(&[(8, CreatureAction::Jump(1))])
}

/// Больше поедания минералов: нижний край.
///
/// Отличий от базы одно: ещё один ген `EatMinerals`.
pub(crate) fn genome_bottom() -> CreatureGenome {
    genome_base().with_genes(&[(8, CreatureAction::EatMinerals)])
}

/// Чаще делится: левый край.
///
/// Отличий от базы одно: ещё один ген `Split`.
pub(crate) fn genome_left() -> CreatureGenome {
    genome_base().with_genes(&[(8, CreatureAction::Split)])
}

/// Размещает одинаковые геномы в указанных клетках поля.
#[cfg(test)]
fn spawn(jar: &mut Jar, genome: &CreatureGenome, positions: &[(usize, usize)]) {
    let size = jar.size;
    let config = jar.config;
    for &(x, y) in positions {
        let mut creature = Creature::new(genome.clone());
        creature.lifespan = random_lifespan(&mut jar.rng, &config);
        jar.creatures[x + y * size] = Some(creature);
    }
}

/// Создает Jar с четырьмя группами существ в случайных местах поля.
///
/// Стартовые позиции случайны: ничьи ниши нигде не заданы, колонии
/// сами находят свет и сталкиваются друг с другом.
pub(crate) fn seeded_jar() -> Jar {
    let mut jar = Jar::new(100);
    let config = jar.config;

    // По три клетки каждого вида: по двадцать четыре гена на банку.
    for (genome, count) in [
        (genome_top(), 3usize),
        (genome_right(), 3),
        (genome_bottom(), 3),
        (genome_left(), 3),
    ] {
        for _ in 0..count {
            let index = jar.random_free_index();
            let mut creature = Creature::new(genome.clone());
            creature.lifespan = random_lifespan(&mut jar.rng, &config);
            jar.creatures[index] = Some(creature);
        }
    }

    jar
}

// ===========================================================================
// Интерфейс: камера, ввод, часы симуляции
// ===========================================================================

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

/// Скорость симуляции: либо несколько тиков за кадр, либо один тик раз в несколько кадров.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// Команда, которую может выполнить приложение.
///
/// Команды не знают про клавиши: одна и та же команда может прийти
/// с клавиатуры, с мыши или из теста.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Command {
    /// Поставить симуляцию на паузу или снять ее.
    TogglePause,
    /// Переключить режим просмотра поля.
    NextView,
    /// Вписать поле в окно целиком.
    FitZoom,
    /// Ускорить симуляцию.
    SpeedUp,
    /// Замедлить симуляцию.
    SpeedDown,
    /// Изменить масштаб на указанное число ступеней.
    Zoom(i32),
    /// Сдвинуть выделение на одну клетку.
    Move(Direction),
}

/// Команда по нажатой клавише.
///
/// Клавиши перемещения сюда не попадают: у них есть нажатие и отпускание,
/// поэтому ими занимается [`KeyRepeat`].
fn command_from_key(key: PhysicalKey) -> Option<Command> {
    match key {
        PhysicalKey::Code(KeyCode::Space) => Some(Command::TogglePause),
        PhysicalKey::Code(KeyCode::Tab) => Some(Command::NextView),
        PhysicalKey::Code(KeyCode::KeyF) => Some(Command::FitZoom),
        PhysicalKey::Code(KeyCode::Equal | KeyCode::NumpadAdd) => Some(Command::SpeedUp),
        PhysicalKey::Code(KeyCode::Minus | KeyCode::NumpadSubtract) => Some(Command::SpeedDown),
        _ => None,
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

/// Нажатая клавиша перемещения и момент нажатия.
#[derive(Clone, Copy, Debug)]
struct HeldKey {
    direction: Direction,
    since: Instant,
}

/// Автоповтор перемещения выделения.
///
/// Клавиш может быть нажато несколько: удержание одной не отменяет
/// удержание другой, поэтому набор хранится в порядке нажатия.
#[derive(Debug)]
struct KeyRepeat {
    /// Нажатые клавиши, от самой старой к самой новой.
    held: Vec<HeldKey>,
    /// Когда сработает следующий автоповтор.
    next_repeat: Instant,
}

impl KeyRepeat {
    /// Создает автоповтор, готовый сработать немедленно.
    fn new(now: Instant) -> Self {
        Self {
            held: Vec::new(),
            next_repeat: now,
        }
    }

    /// Клавиша нажата: первый сдвиг и запуск автоповтора.
    fn press(&mut self, direction: Direction, now: Instant) {
        if !self.held.iter().any(|key| key.direction == direction) {
            self.held.push(HeldKey {
                direction,
                since: now,
            });
        }
        self.next_repeat = now + REPEAT_DELAY;
    }

    /// Клавиша отпущена: автоповтор по ней прекращается.
    ///
    /// Остальные нажатые клавиши продолжают работать.
    fn release(&mut self, direction: Direction, now: Instant) {
        self.held.retain(|key| key.direction != direction);
        if !self.held.is_empty() {
            // Новая клавиша начинает свой отсчет заново.
            self.next_repeat = now + REPEAT_DELAY;
        }
    }

    /// Все нажатые клавиши отпущены.
    fn release_all(&mut self) {
        self.held.clear();
    }

    /// Удерживается ли хоть одна клавиша.
    fn is_held(&self) -> bool {
        !self.held.is_empty()
    }

    /// Когда сработает следующий автоповтор.
    fn next_repeat_at(&self) -> Option<Instant> {
        self.held.last().map(|_| self.next_repeat)
    }

    /// Автоповтор: пока клавиша удерживается, выделение едет.
    ///
    /// Возвращает направление, в котором нужно сдвинуть выделение.
    fn poll(&mut self, now: Instant) -> Option<Direction> {
        let key = *self.held.last()?;
        if now < self.next_repeat {
            return None;
        }

        self.next_repeat = now + repeat_interval(now.duration_since(key.since));
        Some(key.direction)
    }
}

/// Часы симуляции: скорость, пауза и расписание тиков.
///
/// Отдельный тип от [`App`], потому что расписание тиков — это
/// чистая арифметика, которую можно проверить без окна.
#[derive(Debug)]
struct SimClock {
    /// Скорость симуляции.
    speed: Speed,
    /// Сколько кадров накоплено при замедленном режиме.
    frame_accum: u32,
    /// Приостановлена ли симуляция.
    paused: bool,
    /// Когда будет следующий кадр симуляции.
    next_frame: Instant,
}

impl SimClock {
    /// Создает часы, готовые к первому кадру.
    fn new(now: Instant) -> Self {
        Self {
            speed: Speed::TicksPerFrame(1),
            frame_accum: 0,
            paused: false,
            next_frame: now,
        }
    }

    /// Сколько тиков нужно выполнить в этом кадре.
    ///
    /// Заодно назначает время следующего кадра.
    fn ticks_due(&mut self, now: Instant) -> u32 {
        if self.paused || now < self.next_frame {
            return 0;
        }

        self.next_frame = now + FRAME_TIME;

        match self.speed {
            Speed::TicksPerFrame(n) => n,
            Speed::FramesPerTick(n) => {
                self.frame_accum += 1;
                if self.frame_accum >= n {
                    self.frame_accum = 0;
                    1
                } else {
                    0
                }
            }
        }
    }

    /// Ставит симуляцию на паузу или снимает ее.
    fn toggle_pause(&mut self, now: Instant) {
        self.paused = !self.paused;

        if !self.paused {
            // Возобновляем симуляцию сразу, не дожидаясь следующего кадра.
            self.next_frame = now;
        }
    }

    /// Меняет скорость симуляции.
    fn set_speed(&mut self, speed: Speed) {
        if speed != self.speed {
            self.speed = speed;
            self.frame_accum = 0;
        }
    }

    /// Подпись скорости для заголовка окна.
    fn speed_label(&self) -> String {
        self.speed.label()
    }
}

/// Камера: размер клетки, окно и положение выделения.
#[derive(Debug)]
struct Camera {
    /// Текущий размер клетки в пикселях.
    cell_size: u32,
    /// Размер клетки, при котором поле целиком влезает в окно.
    fit_cell_size: u32,
    /// Размер окна в пикселях: нужен, чтобы знать, помещается ли поле.
    window_size: (u32, u32),
    /// Позиция выбранной клетки.
    cursor_x: usize,
    cursor_y: usize,
    /// Размер поля в клетках.
    world_size: usize,
}

impl Camera {
    /// Создает камеру с видом на верхнюю колонию.
    fn new(world_size: usize, window_size: (u32, u32)) -> Self {
        let fit_cell_size = fit_cell_size(window_size.0, window_size.1, world_size);

        Self {
            cell_size: fit_cell_size,
            fit_cell_size,
            window_size,
            cursor_x: world_size / 2,
            cursor_y: 2.min(world_size.saturating_sub(1)),
            world_size,
        }
    }

    /// Запоминает новое окно и пересчитывает вписанный размер клетки.
    fn set_window_size(&mut self, width: u32, height: u32) {
        self.window_size = (width, height);
        self.fit_cell_size = fit_cell_size(width, height, self.world_size);
    }

    /// Меняет масштаб на указанное число ступеней.
    ///
    /// Ступени берутся из общей шкалы [`ZOOM_LEVELS`], поэтому зум
    /// всегда обратим: шаг туда и шаг обратно дают тот же размер.
    fn zoom(&mut self, steps: i32) {
        let level = zoom_level(self.cell_size);
        let target = (level as i32 + steps).clamp(0, ZOOM_LEVELS.len() as i32 - 1) as usize;
        self.set_cell_size(ZOOM_LEVELS[target]);
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
        let was_fitting = field_fits(self.cell_size, self.world_size, width, height);
        let will_fit = field_fits(new_size, self.world_size, width, height);

        if was_fitting && !will_fit {
            // Поле перестало помещаться: выделение появляется в центре экрана,
            // чтобы камера осталась на месте.
            let (x, y) = self.center_cell();
            self.cursor_x = x;
            self.cursor_y = y;
        }

        self.cell_size = new_size;
    }

    /// Существует ли сейчас выделение клетки.
    ///
    /// Пока поле целиком помещается в окне, выделения нет: камера центрирует
    /// поле, курсор не нужен.
    fn selection_active(&self) -> bool {
        !field_fits(
            self.cell_size,
            self.world_size,
            self.window_size.0,
            self.window_size.1,
        )
    }

    /// Клетка, оказавшаяся в центре текущего вида.
    fn center_cell(&self) -> (usize, usize) {
        let (offset_x, offset_y) = field_offset(
            (self.cursor_x, self.cursor_y),
            self.world_size,
            self.cell_size,
            self.window_size.0,
            self.window_size.1,
        );

        let cell = self.cell_size.max(1) as i64;
        let last = self.world_size.saturating_sub(1) as i64;

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
        let last = self.world_size.saturating_sub(1) as isize;

        self.cursor_x = (self.cursor_x as isize + dx).clamp(0, last) as usize;
        self.cursor_y = (self.cursor_y as isize + dy).clamp(0, last) as usize;
    }
}

/// Помещается ли поле целиком в окно при таком размере клетки.
fn field_fits(cell_size: u32, size: usize, width: u32, height: u32) -> bool {
    let grid = size as u32 * cell_size;
    grid <= width && grid <= height
}

/// Размер клетки, при котором поле целиком влезает в окно.
///
/// Берутся только ступени общей шкалы зума: так вписанный вид
/// всегда совпадает с одним из режимов масштаба.
fn fit_cell_size(width: u32, height: u32, size: usize) -> u32 {
    let limit = width.min(height) / size.max(1) as u32;

    let mut best = MIN_CELL_SIZE;
    for &level in ZOOM_LEVELS.iter() {
        if level <= limit {
            best = level;
        }
    }

    best.clamp(MIN_CELL_SIZE, MAX_CELL_SIZE)
}

/// Номер ступени зума для размера клетки.
fn zoom_level(cell_size: u32) -> usize {
    let mut best = 0;
    let mut best_distance = u32::MAX;

    for (index, &level) in ZOOM_LEVELS.iter().enumerate() {
        let distance = level.abs_diff(cell_size);
        if distance < best_distance {
            best_distance = distance;
            best = index;
        }
    }

    best
}

// ===========================================================================
// Отрисовка
// ===========================================================================

/// Режим просмотра поля.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// Все режимы в порядке переключения.
    pub(crate) const ALL: [ViewMode; 6] = [
        Self::Creatures,
        Self::Minerals,
        Self::Energy,
        Self::Light,
        Self::Genome,
        Self::Action,
    ];

    /// Следующий режим по кругу.
    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// Описание режима: как его показывать.
    fn spec(self) -> &'static ViewSpec {
        &VIEW_SPECS[Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0)]
    }

    /// Подпись режима для заголовка окна.
    pub(crate) fn label(self) -> &'static str {
        self.spec().label
    }
}

/// Дополнительный слой поверх клеток в некотором режиме.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Overlay {
    /// Затемнение по освещенности поля.
    Light,
}

/// Декор внутри клетки в некотором режиме.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decor {
    /// Полоска состава генома внизу клетки.
    GenomeBar,
}

/// Как нарисовать клетку в этом режиме.
type ColorFn = fn(&JarSnapshot, usize, &Palette) -> u32;

/// Как нарисовать легенду шкалы в этом режиме.
type LegendFn = fn(&JarSnapshot) -> (&'static [Stop], usize);

/// Описание режима просмотра.
///
/// Новый режим просмотра — это одна запись здесь плюс функция цвета:
/// больше нигде не нужно вспоминать про конкретные режимы.
struct ViewSpec {
    /// Подпись режима.
    label: &'static str,
    /// Нужна ли сетка: без нее не видно границы клеток.
    grid: bool,
    /// Рисуется ли контур колоний вместо сетки.
    outline: bool,
    /// Сглаживается ли цвет клетки между кадрами.
    trails: bool,
    /// Цвет клетки в этом режиме.
    color: ColorFn,
    /// Легенда шкалы, если она есть.
    legend: Option<LegendFn>,
    /// Слой поверх клеток, если он есть.
    overlay: Option<Overlay>,
    /// Декор внутри клетки, если он есть.
    decor: Option<Decor>,
}

/// Описания всех режимов просмотра.
static VIEW_SPECS: [ViewSpec; 6] = [
    ViewSpec {
        label: "creatures",
        grid: false,
        outline: true,
        trails: false,
        color: color_creatures,
        legend: None,
        overlay: None,
        decor: None,
    },
    ViewSpec {
        label: "minerals",
        grid: true,
        outline: false,
        trails: true,
        color: color_minerals,
        legend: Some(legend_minerals),
        overlay: None,
        decor: None,
    },
    ViewSpec {
        label: "energy",
        grid: true,
        outline: false,
        trails: true,
        color: color_energy,
        legend: Some(legend_energy),
        overlay: None,
        decor: None,
    },
    ViewSpec {
        label: "light",
        grid: false,
        outline: true,
        trails: false,
        color: color_light,
        legend: None,
        overlay: Some(Overlay::Light),
        decor: None,
    },
    ViewSpec {
        label: "genome",
        grid: true,
        outline: false,
        trails: false,
        color: color_genome,
        legend: None,
        overlay: None,
        decor: Some(Decor::GenomeBar),
    },
    ViewSpec {
        label: "action",
        grid: true,
        outline: false,
        trails: false,
        color: color_action,
        legend: None,
        overlay: None,
        decor: None,
    },
];

/// Цвет клетки в режиме существ.
fn color_creatures(snapshot: &JarSnapshot, index: usize, palette: &Palette) -> u32 {
    if !snapshot.cells[index].alive {
        return palette.jar_bg;
    }
    // Чем больше энергии, тем светлее клетка.
    let energy = snapshot.cells[index].energy.min(100) as u32;
    mix(palette.creature, 0x00FF_FFFF, energy * 80 / 100)
}

/// Цвет клетки в режиме освещенности: тот же цвет, что и в режиме существ.
fn color_light(snapshot: &JarSnapshot, index: usize, palette: &Palette) -> u32 {
    color_creatures(snapshot, index, palette)
}

/// Цвет клетки в режиме минералов.
fn color_minerals(snapshot: &JarSnapshot, index: usize, _palette: &Palette) -> u32 {
    let max = snapshot.max_minerals.max(1) as u32;
    let amount = snapshot.minerals[index].min(max as usize) as u32;
    ramp(amount * 255 / max, &MINERAL_RAMP)
}

/// Цвет клетки в режиме энергии.
fn color_energy(snapshot: &JarSnapshot, index: usize, palette: &Palette) -> u32 {
    if !snapshot.cells[index].alive {
        return palette.jar_bg;
    }
    let max = snapshot.max_energy.max(1) as u32;
    let amount = snapshot.cells[index].energy.min(max as usize) as u32;
    ramp(amount * 255 / max, &ENERGY_RAMP)
}

/// Цвет клетки в режиме геномов.
fn color_genome(snapshot: &JarSnapshot, index: usize, palette: &Palette) -> u32 {
    snapshot.cells[index].genome_color.unwrap_or(palette.jar_bg)
}

/// Цвет клетки в режиме действий.
fn color_action(snapshot: &JarSnapshot, index: usize, palette: &Palette) -> u32 {
    match snapshot.cells[index].action {
        Some(action) => palette.action_colors[action.kind().index()],
        None => palette.jar_bg,
    }
}

/// Легенда шкалы минералов.
fn legend_minerals(snapshot: &JarSnapshot) -> (&'static [Stop], usize) {
    (&MINERAL_RAMP, snapshot.max_minerals)
}

/// Легенда шкалы энергии.
fn legend_energy(snapshot: &JarSnapshot) -> (&'static [Stop], usize) {
    (&ENERGY_RAMP, snapshot.max_energy)
}

/// Цвет клетки в текущем режиме просмотра.
fn cell_color(snapshot: &JarSnapshot, index: usize, view: ViewMode, palette: &Palette) -> u32 {
    (view.spec().color)(snapshot, index, palette)
}

/// Положение панели относительно окна.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anchor {
    /// Левый верхний угол.
    TopLeft,
    /// Правый верхний угол.
    TopRight,
    /// Левый нижний угол.
    BottomLeft,
    /// Правый нижний угол.
    BottomRight,
}

impl Anchor {
    /// Положение панели размером `size` в окне `window` с отступом `margin`.
    fn place(self, window: (u32, u32), size: (u32, u32), margin: u32) -> (i64, i64) {
        let x = match self {
            Self::TopLeft | Self::BottomLeft => margin as i64,
            Self::TopRight | Self::BottomRight => window.0 as i64 - size.0 as i64 - margin as i64,
        };
        let y = match self {
            Self::TopLeft | Self::TopRight => margin as i64,
            Self::BottomLeft | Self::BottomRight => window.1 as i64 - size.1 as i64 - margin as i64,
        };
        (x, y)
    }
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

    /// Рисует рамку вокруг прямоугольника толщиной в `thickness` пикселей.
    fn stroke_rect(
        &mut self,
        x: i64,
        y: i64,
        rect_width: u32,
        rect_height: u32,
        thickness: u32,
        color: u32,
    ) {
        let thickness = thickness.max(1);

        self.fill_rect(x, y, rect_width, thickness, color);
        self.fill_rect(
            x,
            y + rect_height as i64 - thickness as i64,
            rect_width,
            thickness,
            color,
        );
        self.fill_rect(x, y, thickness, rect_height, color);
        self.fill_rect(
            x + rect_width as i64 - thickness as i64,
            y,
            thickness,
            rect_height,
            color,
        );
    }

    /// Рисует четыре уголка по краям квадрата со стороной `size`.
    ///
    /// Содержимое квадрата не закрашивается: рамка только обозначает границы.
    fn corner_brackets(&mut self, x: i64, y: i64, size: i64, arm: i64, thickness: i64, color: u32) {
        let arm = arm.max(1).min(size.max(1));
        let thickness = thickness.max(1);

        // Левый верхний угол.
        self.fill_rect(x, y, arm as u32, thickness as u32, color);
        self.fill_rect(x, y, thickness as u32, arm as u32, color);
        // Правый верхний.
        self.fill_rect(x + size - arm, y, arm as u32, thickness as u32, color);
        self.fill_rect(x + size - thickness, y, thickness as u32, arm as u32, color);
        // Левый нижний.
        self.fill_rect(x, y + size - thickness, arm as u32, thickness as u32, color);
        self.fill_rect(x, y + size - arm, thickness as u32, arm as u32, color);
        // Правый нижний.
        self.fill_rect(
            x + size - arm,
            y + size - thickness,
            arm as u32,
            thickness as u32,
            color,
        );
        self.fill_rect(
            x + size - thickness,
            y + size - arm,
            thickness as u32,
            arm as u32,
            color,
        );
    }

    /// Читает пиксель. За пределами окна возвращает `None`.
    fn pixel_at(&self, x: i64, y: i64) -> Option<u32> {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return None;
        }
        self.buffer
            .get((y as u32 * self.width + x as u32) as usize)
            .copied()
    }

    /// Рисует один пиксель.
    fn pixel(&mut self, x: i64, y: i64, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }

        self.buffer[(y as u32 * self.width + x as u32) as usize] = color;
    }

    /// Перекрашивает пиксель в цвет, посчитанный от старого.
    ///
    /// За пределами окна ничего не происходит: буфер никогда не
    /// индексируется вслепую.
    fn shade_pixel(&mut self, x: i64, y: i64, shade: impl Fn(u32) -> u32) {
        let Some(current) = self.pixel_at(x, y) else {
            return;
        };
        self.pixel(x, y, shade(current));
    }

    /// Заливает весь буфер цветом.
    fn fill(&mut self, color: u32) {
        self.buffer.fill(color);
    }
}

/// Положение поля в окне: смещение, размер клетки и размер поля.
struct Viewport {
    /// Смещение поля по горизонтали в пикселях.
    offset_x: i64,
    /// Смещение поля по вертикали в пикселях.
    offset_y: i64,
    /// Размер клетки в пикселях.
    cell: u32,
    /// Размер поля в пикселях.
    grid: u32,
    /// Размер поля в клетках.
    size: usize,
}

impl Viewport {
    /// Левый верхний угол клетки в пикселях окна.
    fn cell_origin(&self, x: usize, y: usize) -> (i64, i64) {
        (
            self.offset_x + (x as u32 * self.cell) as i64,
            self.offset_y + (y as u32 * self.cell) as i64,
        )
    }

    /// Диапазон клеток, которые попадают в окно: `(x0, x1, y0, y1)`.
    ///
    /// Нужен, чтобы не закрашивать все десять тысяч клеток,
    /// когда в окно видно только сотню.
    fn visible_cells(&self, width: u32, height: u32) -> (usize, usize, usize, usize) {
        let cell = self.cell.max(1) as i64;

        let x0 = ((-self.offset_x).max(0) / cell) as usize;
        let y0 = ((-self.offset_y).max(0) / cell) as usize;
        let x1 = (((width as i64 - self.offset_x).max(0) + cell - 1) / cell) as usize;
        let y1 = (((height as i64 - self.offset_y).max(0) + cell - 1) / cell) as usize;

        (
            x0.min(self.size),
            x1.min(self.size),
            y0.min(self.size),
            y1.min(self.size),
        )
    }
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
    mix: Option<[u8; ActionKind::COUNT]>,
    /// Последнее выполненное действие.
    action: Option<CreatureAction>,
}

impl CellInfo {
    /// Собирает данные клетки из снимка мира.
    fn from_snapshot(snapshot: &JarSnapshot, x: usize, y: usize) -> Self {
        let index = Grid::new(snapshot.size).idx(x, y);
        let cell = snapshot
            .cells
            .get(index)
            .copied()
            .unwrap_or(CellView::EMPTY);

        Self {
            x,
            y,
            alive: cell.alive,
            energy: cell.energy,
            minerals: snapshot.minerals.get(index).copied().unwrap_or(0),
            eaten: cell.eaten,
            age: cell.age,
            mix: cell.genome_mix,
            action: cell.action,
        }
    }
}

/// Строки панели HUD.
fn hud_lines(
    snapshot: &JarSnapshot,
    view: ViewMode,
    speed: &str,
    zoom: &str,
    paused: bool,
) -> [String; 2] {
    let minerals: usize = snapshot.minerals.iter().sum();

    [
        format!(
            "tick {}  pop {}  min {}",
            snapshot.tick,
            snapshot.population,
            format_count(minerals)
        ),
        format!(
            "mode {}  speed {}  zoom {}{}",
            view.label(),
            speed,
            zoom,
            if paused { "  PAUSED" } else { "" }
        ),
    ]
}

/// Строки панели инспектора.
fn inspector_lines(info: &CellInfo) -> Vec<String> {
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

    lines
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
    let mut cell_layer = vec![palette.jar_bg; snapshot.size * snapshot.size];

    draw_frame_with_layer(
        buffer,
        width,
        height,
        snapshot,
        view,
        cell_size,
        cursor,
        &mut cell_layer,
    );
}

/// Рисует кадр, сохраняя предыдущие цвета клеток.
///
/// Слой нужен для сглаживания шкалы: он живет между кадрами,
/// поэтому цвет клетки догоняет новые данные постепенно.
#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn draw_frame_with_layer(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    snapshot: &JarSnapshot,
    view: ViewMode,
    cell_size: u32,
    cursor: (usize, usize),
    cell_layer: &mut Vec<u32>,
) {
    let palette = Palette::new();
    let flashes = vec![0u8; snapshot.size * snapshot.size];
    let info = CellInfo::from_snapshot(snapshot, cursor.0, cursor.1);
    let [hud_top, hud_bottom] = hud_lines(snapshot, view, "x1", "fit", false);

    let mut context = RenderContext {
        snapshot,
        view,
        palette: &palette,
        cell_size,
        flashes: &flashes,
        cell_layer,
        selection_active: !field_fits(cell_size, snapshot.size, width, height),
        cursor,
        info,
        hud_top,
        hud_bottom,
    };

    let mut canvas = Canvas {
        buffer,
        width,
        height,
    };
    draw_snapshot(&mut canvas, &mut context);
}

/// Рисует кадр: слои идут снизу вверх, порядок задан в плане.
fn draw_snapshot(canvas: &mut Canvas, context: &mut RenderContext) {
    let palette = context.palette;
    let snapshot = context.snapshot;
    let spec = context.view.spec();
    let cell = context.cell_size.max(1);
    let grid = snapshot.size as u32 * cell;
    let (offset_x, offset_y) = field_offset(
        context.cursor,
        snapshot.size,
        cell,
        canvas.width,
        canvas.height,
    );

    let viewport = Viewport {
        offset_x,
        offset_y,
        cell,
        grid,
        size: snapshot.size,
    };

    // 0. фон окна вокруг банки
    canvas.fill(palette.window_bg);

    // 1. подложка банки
    canvas.fill_rect(offset_x, offset_y, grid, grid, palette.jar_bg);

    // 2. содержимое клеток
    draw_cells(canvas, context, &viewport);

    // 3. сетка: только там, где без неё не видно границы клеток
    if spec.grid {
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
    if spec.outline {
        draw_colony_outline(canvas, snapshot, &viewport, palette.jar_frame);
    }

    // 5. оверлей поверх клеток
    match spec.overlay {
        Some(Overlay::Light) => draw_light_overlay(canvas, &viewport),
        None => {}
    }

    // 6. рамка банки и внутренняя тень
    draw_jar_frame(canvas, offset_x, offset_y, grid, palette);

    // 7. курсор: пока поле помещается в окно, выделения нет
    if context.selection_active {
        draw_cursor(canvas, context, &viewport);
    }

    // 8. оверлеи интерфейса
    draw_minimap(canvas, context, &viewport);
    draw_hud(canvas, context);
    draw_legend(canvas, context);
    if context.selection_active {
        draw_inspector(canvas, context);
    }
}

/// Рисует содержимое клеток поля.
fn draw_cells(canvas: &mut Canvas, context: &mut RenderContext, viewport: &Viewport) {
    let snapshot = context.snapshot;
    let spec = context.view.spec();
    let palette = context.palette;
    let flashes = context.flashes;
    let layer = &mut *context.cell_layer;
    let size = snapshot.size;

    // Цвета считаем для всех клеток: так сглаживание не зависит от того,
    // какие клетки сейчас видны в окне. Рисуем только видимые.
    for y in 0..size {
        for x in 0..size {
            let index = Grid::new(size).idx(x, y);
            let mut color = (spec.color)(snapshot, index, palette);

            // В режимах-тепкартах цвет плавно догоняет новые данные:
            // скачок минералов или энергии не выглядит как вспышка.
            if spec.trails {
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
        }
    }

    let (x0, x1, y0, y1) = viewport.visible_cells(canvas.width, canvas.height);

    for y in y0..y1 {
        for x in x0..x1 {
            let index = Grid::new(size).idx(x, y);
            let mut color = layer[index];

            // Вспышка рождения или смерти.
            if let Some(flash) = flashes.get(index)
                && *flash > 0
            {
                let strength = (*flash as u32) * FLASH_STRENGTH / FLASH_FRAMES as u32;
                color = mix(color, 0x00FF_FFFF, strength);
            }

            let (px, py) = viewport.cell_origin(x, y);
            canvas.fill_rect(px, py, viewport.cell, viewport.cell, color);

            // В режиме геномов внизу клетки показываем состав ДНК:
            // так видно мутацию, которую не показывает цвет.
            if spec.decor == Some(Decor::GenomeBar) {
                draw_genome_bar(canvas, snapshot, index, px, py, viewport.cell, palette);
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

    let Some(mix) = snapshot.cells[index].genome_mix else {
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
    canvas.fill_rect(
        x,
        y,
        rest,
        bar_height,
        palette.action_colors[ActionKind::COUNT - 1],
    );
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
    viewport: &Viewport,
    color: u32,
) {
    let size = snapshot.size;
    let cell = viewport.cell;

    for y in 0..size {
        for x in 0..size {
            if !snapshot.cells[x + y * size].alive {
                continue;
            }

            let open_above = y == 0 || !snapshot.cells[x + (y - 1) * size].alive;
            let open_below = y + 1 == size || !snapshot.cells[x + (y + 1) * size].alive;
            let open_left = x == 0 || !snapshot.cells[(x - 1) + y * size].alive;
            let open_right = x + 1 == size || !snapshot.cells[(x + 1) + y * size].alive;

            if !open_above && !open_below && !open_left && !open_right {
                continue;
            }

            let (px, py) = viewport.cell_origin(x, y);

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
    canvas.stroke_rect(x, y, grid, grid, JAR_FRAME_THICKNESS, palette.jar_frame);
}

/// Курсор: четыре уголка, чтобы не перекрывать содержимое клетки.
fn draw_cursor(canvas: &mut Canvas, context: &RenderContext, viewport: &Viewport) {
    let (x, y) = context.cursor;
    let index = Grid::new(context.snapshot.size).idx(x, y);
    let under = cell_color(context.snapshot, index, context.view, context.palette);
    let palette = context.palette;
    let color = if luma(under) > 140 {
        palette.cursor_dark
    } else {
        palette.cursor_light
    };

    let cell = viewport.cell;
    let arm = (cell / 3).max(3).min(cell);
    let thickness = if cell < 6 { 1 } else { 2 };
    let (px, py) = viewport.cell_origin(x, y);

    canvas.corner_brackets(px, py, cell as i64, arm as i64, thickness as i64, color);
}

/// Оверлей освещенности.
///
/// Свет считается по той же формуле, что и в симуляции, но в дробных клетках,
/// поэтому картинка совпадает с областями фотосинтеза и остается гладкой.
/// Яркость клеток падает в темноте, а на освещенных краях появляется
/// теплый оттенок солнечного света.
fn draw_light_overlay(canvas: &mut Canvas, viewport: &Viewport) {
    let size = viewport.size;
    let cell = viewport.cell;
    let reach_px = light_reach(size) as u32 * cell;
    let table = build_light_table(reach_px, cell, size);
    // Палитра света готовится один раз на кадр, а не на каждый пиксель.
    let palette: Vec<u32> = (0..=255u8).map(light_color).collect();

    let x0 = viewport.offset_x.max(0).min(canvas.width as i64) as u32;
    let y0 = viewport.offset_y.max(0).min(canvas.height as i64) as u32;
    let x1 = (viewport.offset_x + viewport.grid as i64)
        .max(0)
        .min(canvas.width as i64) as u32;
    let y1 = (viewport.offset_y + viewport.grid as i64)
        .max(0)
        .min(canvas.height as i64) as u32;

    let right = viewport.offset_x + viewport.grid as i64 - 1;
    let bottom = viewport.offset_y + viewport.grid as i64 - 1;

    for py in y0..y1 {
        let dy = (py as i64 - viewport.offset_y)
            .min(bottom - py as i64)
            .max(0);

        for px in x0..x1 {
            let dx = (px as i64 - viewport.offset_x)
                .min(right - px as i64)
                .max(0);
            let distance = dx.min(dy) as usize;
            let intensity = table.get(distance).copied().unwrap_or(0);

            // В темноте клетки почти не видны, на свету — полная яркость.
            let factor = DARK_FACTOR + mul255(intensity as u32, 255 - DARK_FACTOR);
            let warm = mul255(intensity as u32, WARM_STRENGTH);
            let tint = palette[intensity as usize];

            canvas.shade_pixel(px as i64, py as i64, |current| {
                let lit = scale(current, factor);
                mix(lit, tint, warm)
            });
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
    let panel_height = lines.len() as u32 * FONT_HEIGHT + PANEL_PADDING * 2;

    let (x, y) = Anchor::TopLeft.place(
        (canvas.width, canvas.height),
        (panel_width_of(&lines), panel_height),
        PANEL_MARGIN,
    );

    draw_panel(canvas, x, y, &lines, palette);
}

/// Панель инспектора: что находится в выбранной клетке.
fn draw_inspector(canvas: &mut Canvas, context: &RenderContext) {
    let lines = inspector_lines(&context.info);
    let palette = context.palette;
    let panel_height = lines.len() as u32 * FONT_HEIGHT + PANEL_PADDING * 2;

    let references: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (x, y) = Anchor::BottomLeft.place(
        (canvas.width, canvas.height),
        (panel_width_of(&references), panel_height),
        PANEL_MARGIN,
    );

    draw_panel(canvas, x, y, &references, palette);
}

/// Легенда шкалы для режимов-тепкарт.
fn draw_legend(canvas: &mut Canvas, context: &RenderContext) {
    let Some(legend) = context.view.spec().legend else {
        return;
    };
    let (stops, max_value) = legend(context.snapshot);

    let label_height = FONT_HEIGHT + 2;
    let (x, y) = Anchor::BottomRight.place(
        (canvas.width, canvas.height),
        (LEGEND_WIDTH, LEGEND_HEIGHT + label_height),
        PANEL_MARGIN,
    );

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
    let label_x = x + LEGEND_WIDTH as i64 - max_label.chars().count() as i64 * TEXT_ADVANCE as i64;
    draw_text(
        canvas,
        label_x,
        y + LEGEND_HEIGHT as i64 + 2,
        &max_label,
        palette.panel_text,
    );
}

/// Мини-карта: вся банка целиком и рамка текущего вида.
fn draw_minimap(canvas: &mut Canvas, context: &RenderContext, viewport: &Viewport) {
    let snapshot = context.snapshot;
    let size = snapshot.size;
    if size == 0 || MINIMAP_SIZE == 0 {
        return;
    }

    let (x, y) = Anchor::TopRight.place(
        (canvas.width, canvas.height),
        (MINIMAP_SIZE, MINIMAP_SIZE),
        PANEL_MARGIN,
    );

    // Фон мини-карты и точки существ.
    canvas.fill_rect(x, y, MINIMAP_SIZE, MINIMAP_SIZE, context.palette.jar_bg);

    // Мини-карта всегда квадратная: поле размера `size` рисуется в нее
    // целиком, поэтому карта не вылезает за рамку ни при каком размере поля.
    for cy in 0..size {
        for cx in 0..size {
            if !snapshot.cells[cx + cy * size].alive {
                continue;
            }
            let (dx, dy) = minimap_position(cx, cy, size);
            canvas.fill_rect(x + dx, y + dy, 1, 1, context.palette.creature);
        }
    }

    // Рамка текущего вида: какие клетки поля попали в окно.
    let cell_i = viewport.cell.max(1) as i64;
    let left = ((-viewport.offset_x) / cell_i).clamp(0, size as i64) as usize;
    let top = ((-viewport.offset_y) / cell_i).clamp(0, size as i64) as usize;
    let right = ((canvas.width as i64 - viewport.offset_x) / cell_i).clamp(0, size as i64) as usize;
    let bottom =
        ((canvas.height as i64 - viewport.offset_y) / cell_i).clamp(0, size as i64) as usize;

    let (view_x, view_y) = minimap_position(left, top, size);
    let (view_right, view_bottom) = minimap_position(right, bottom, size);
    let view_width = ((view_right - view_x) as u32).max(1);
    let view_height = ((view_bottom - view_y) as u32).max(1);

    canvas.stroke_rect(
        x + view_x,
        y + view_y,
        view_width,
        view_height,
        1,
        context.palette.cursor_dark,
    );

    // Внешняя рамка мини-карты.
    canvas.stroke_rect(
        x - 1,
        y - 1,
        MINIMAP_SIZE + 2,
        MINIMAP_SIZE + 2,
        1,
        context.palette.panel_border,
    );
}

/// Положение клетки поля на мини-карте.
///
/// Мини-карта всегда квадратная и никогда не больше [`MINIMAP_SIZE`]:
/// поэтому поле любого размера помещается в нее целиком.
fn minimap_position(cell_x: usize, cell_y: usize, size: usize) -> (i64, i64) {
    let map = MINIMAP_SIZE as usize;
    ((cell_x * map / size) as i64, (cell_y * map / size) as i64)
}

/// Ширина текста панели в пикселях.
///
/// Считается по символам, а не по байтам: кириллица в UTF-8
/// занимает два байта, но рисуется одним глифом.
fn panel_text_width<'a, I: Iterator<Item = &'a str>>(lines: I) -> u32 {
    lines
        .map(|line| line.chars().count() as u32 * TEXT_ADVANCE)
        .max()
        .unwrap_or(0)
}

/// Ширина панели в пикселях.
fn panel_width<'a, I: Iterator<Item = &'a str>>(lines: I) -> u32 {
    panel_text_width(lines) + PANEL_PADDING * 2
}

/// Ширина панели по готовому срезу строк.
fn panel_width_of(lines: &[&str]) -> u32 {
    panel_width(lines.iter().copied())
}

/// Панель с текстом на светлом фоне.
fn draw_panel(canvas: &mut Canvas, x: i64, y: i64, lines: &[&str], palette: &Palette) {
    let panel_width = panel_width_of(lines);
    let panel_height = lines.len() as u32 * FONT_HEIGHT + PANEL_PADDING * 2;

    canvas.fill_rect(x, y, panel_width, panel_height, palette.panel_bg);

    // Рамка панели.
    canvas.stroke_rect(x, y, panel_width, panel_height, 1, palette.panel_border);

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
        Some(action) => action.kind().label(),
        None => "-",
    }
}

/// Подпись состава генома: доли типов генов в процентах.
fn mix_label(mix: Option<[u8; ActionKind::COUNT]>) -> String {
    let Some(mix) = mix else {
        return "-".to_string();
    };

    let parts: Vec<String> = ActionKind::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| format!("{} {}%", kind.label(), mix[index] as u32 * 100 / 255))
        .collect();

    parts.join(" ")
}

/// Короткая запись больших чисел: 42000 -> 42.0k.
fn format_count(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}m", value as f32 / 1_000_000.0)
    } else if value >= 10_000 {
        let thousands = value as f32 / 1_000.0;
        // 999999 должен показываться как 1.0m, а не как 1000.0k.
        if thousands >= 999.95 {
            format!("{:.1}m", thousands / 1_000.0)
        } else {
            format!("{:.1}k", thousands)
        }
    } else {
        value.to_string()
    }
}

/// Цвет по рампе: `t` в диапазоне 0..=255.
fn ramp(t: u32, stops: &[Stop]) -> u32 {
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
    ramp(intensity as u32, &LIGHT_RAMP)
}

// ===========================================================================
// Приложение
// ===========================================================================

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

/// Окно вместе с поверхностью для рисования.
type WindowStack = (
    Arc<Window>,
    Context<Arc<Window>>,
    Surface<Arc<Window>, Arc<Window>>,
);

/// Создает окно вместе с поверхностью для рисования.
///
/// Все три шага могут провалиться, поэтому ошибка собирается в одно
/// сообщение: вызывающий код только печатает его и выходит.
fn create_window_stack(event_loop: &ActiveEventLoop) -> Result<WindowStack, String> {
    let attributes = Window::default_attributes()
        .with_title("Cells")
        .with_inner_size(LogicalSize::new(800.0, 800.0))
        .with_min_inner_size(LogicalSize::new(320.0, 320.0));

    let window = event_loop
        .create_window(attributes)
        .map(Arc::new)
        .map_err(|err| format!("Не удалось создать окно: {err}"))?;

    let context = Context::new(window.clone())
        .map_err(|err| format!("Не удалось создать softbuffer context: {err}"))?;

    let surface = Surface::new(&context, window.clone())
        .map_err(|err| format!("Не удалось создать softbuffer surface: {err}"))?;

    Ok((window, context, surface))
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
    /// Камера: размер клетки, окно и положение выделения.
    camera: Camera,
    /// Автоповтор перемещения выделения.
    keys: KeyRepeat,
    /// Часы симуляции: скорость, пауза и расписание тиков.
    clock: SimClock,
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
}

impl App {
    fn new(jar: Jar) -> Self {
        let mut snapshot = JarSnapshot::new(jar.size);
        jar.snapshot_into(&mut snapshot);

        let size = jar.size;
        let palette = Palette::new();
        let now = Instant::now();

        Self {
            flashes: vec![0; size * size],
            cell_layer: vec![palette.jar_bg; size * size],
            prev_creatures: snapshot.cells.iter().map(|cell| cell.alive).collect(),
            snapshot,
            jar,
            window: None,
            context: None,
            surface: None,
            palette,
            // Стартовая камера смотрит на верхнюю колонию: в центре банки никого нет.
            camera: Camera::new(size, (800, 800)),
            keys: KeyRepeat::new(now),
            clock: SimClock::new(now),
            view_mode: ViewMode::Creatures,
            last_title: now - TITLE_INTERVAL,
            title: String::new(),
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Текст заголовка окна.
    fn title_text(&self) -> String {
        let pause = if self.clock.paused { " | paused" } else { "" };
        format!(
            "Cells | tick {} | pop {} | zoom {} | speed {} | mode {}{}",
            self.snapshot.tick,
            self.snapshot.population,
            self.zoom_label(),
            self.clock.speed_label(),
            self.view_mode.label(),
            pause
        )
    }

    /// Масштаб относительно вписанного вида.
    fn zoom_label(&self) -> String {
        if !self.camera.selection_active() {
            return "fit".to_string();
        }

        let fit = self.camera.fit_cell_size.max(1) as f32;
        format!("x{:.1}", self.camera.cell_size as f32 / fit)
    }

    /// Обновляет заголовок окна, но не чаще чем раз в `TITLE_INTERVAL`.
    fn update_title(&mut self) {
        let Some(window) = self.window.clone() else {
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

    /// Выполняет команду.
    fn apply(&mut self, command: Command) {
        match command {
            Command::TogglePause => self.toggle_pause(),
            Command::NextView => self.toggle_view_mode(),
            Command::FitZoom => self.fit_zoom(),
            Command::SpeedUp => self.change_speed(1),
            Command::SpeedDown => self.change_speed(-1),
            Command::Zoom(steps) => self.zoom(steps),
            Command::Move(direction) => {
                self.camera.move_selection(direction);
                self.request_redraw();
            }
        }
    }

    /// Меняет скорость симуляции: delta > 0 — быстрее, delta < 0 — медленнее.
    fn change_speed(&mut self, delta: i32) {
        let new_speed = if delta > 0 {
            self.clock.speed.faster()
        } else if delta < 0 {
            self.clock.speed.slower()
        } else {
            self.clock.speed
        };

        if new_speed != self.clock.speed {
            self.clock.set_speed(new_speed);
            self.update_title();
        }
    }

    /// Ставит симуляцию на паузу или снимает ее.
    fn toggle_pause(&mut self) {
        self.clock.toggle_pause(Instant::now());
        self.update_title();
    }

    /// Переключает режим просмотра поля.
    fn toggle_view_mode(&mut self) {
        self.view_mode = self.view_mode.next();
        self.update_title();
        self.request_redraw();
    }

    fn zoom(&mut self, steps: i32) {
        self.camera.zoom(steps);
        self.update_title();
        self.request_redraw();
    }

    /// Вписывает поле в окно целиком.
    fn fit_zoom(&mut self) {
        self.camera.fit_zoom();
        self.update_title();
        self.request_redraw();
    }

    /// Клавиша движения нажата: первый сдвиг и запуск автоповтора.
    fn press_direction(&mut self, direction: Direction) {
        let now = Instant::now();
        self.keys.press(direction, now);
        self.apply(Command::Move(direction));
    }

    /// Клавиша движения отпущена: автоповтор по ней прекращается.
    fn release_direction(&mut self, direction: Direction) {
        self.keys.release(direction, Instant::now());
    }

    /// Пересчитывает вспышки рождения и смерти по разнице двух кадров.
    fn update_flashes(&mut self) {
        for (index, cell) in self.snapshot.cells.iter().enumerate() {
            let was_alive = self.prev_creatures[index];
            if cell.alive != was_alive {
                self.flashes[index] = FLASH_FRAMES;
            } else if self.flashes[index] > 0 {
                self.flashes[index] -= 1;
            }
        }

        for (index, cell) in self.snapshot.cells.iter().enumerate() {
            self.prev_creatures[index] = cell.alive;
        }
    }

    /// Данные выбранной клетки для инспектора.
    fn cell_info(&self) -> CellInfo {
        let (x, y) = (self.camera.cursor_x, self.camera.cursor_y);
        CellInfo::from_snapshot(&self.snapshot, x, y)
    }

    /// Обновляет снимок мира и вспышки после тиков симуляции.
    fn refresh_snapshot(&mut self) {
        self.jar.snapshot_into(&mut self.snapshot);
        self.update_flashes();
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
        let selection_active = self.camera.selection_active();
        let info = self.cell_info();
        let [hud_top, hud_bottom] = hud_lines(
            &self.snapshot,
            self.view_mode,
            &self.clock.speed_label(),
            &self.zoom_label(),
            self.clock.paused,
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
                cell_size: self.camera.cell_size,
                flashes: &self.flashes,
                cell_layer: &mut self.cell_layer,
                selection_active,
                cursor: (self.camera.cursor_x, self.camera.cursor_y),
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

        let stack = match create_window_stack(event_loop) {
            Ok(stack) => stack,
            Err(err) => {
                eprintln!("{err}");
                event_loop.exit();
                return;
            }
        };

        let size = stack.0.inner_size();
        self.camera.set_window_size(size.width, size.height);
        self.camera.fit_zoom();

        self.window = Some(stack.0);
        self.context = Some(stack.1);
        self.surface = Some(stack.2);

        self.request_redraw();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.surface = None;
        self.context = None;
        self.window = None;
        // Клавиша отпускания может не прийти: останавливаем автоповтор.
        self.keys.release_all();
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
                self.camera.set_window_size(size.width, size.height);
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Focused(false) => {
                // При потере фокуса окно не пришлет отпускание клавиши.
                self.keys.release_all();
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
                        ElementState::Released => self.release_direction(direction),
                    }
                    return;
                }

                if event.state == ElementState::Pressed
                    && let Some(command) = command_from_key(event.physical_key)
                {
                    self.apply(command);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 50.0,
                };

                if scroll > 0.0 {
                    self.apply(Command::Zoom(1));
                } else if scroll < 0.0 {
                    self.apply(Command::Zoom(-1));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        // Автоповтор перемещения работает и на паузе.
        if let Some(direction) = self.keys.poll(now) {
            self.camera.move_selection(direction);
            self.request_redraw();
        }

        if self.clock.paused {
            // На паузе спим, но если клавиша удерживается, просыпаемся ради неё.
            if self.keys.is_held() {
                let next_repeat = self.keys.next_repeat_at().unwrap_or(now + REPEAT_DELAY);
                event_loop.set_control_flow(ControlFlow::WaitUntil(next_repeat));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            return;
        }

        let ticks = self.clock.ticks_due(now);
        if ticks > 0 {
            self.run_ticks(ticks);
            self.refresh_snapshot();
            self.request_redraw();
        }

        // Просыпаемся на кадр симуляции или на момент автоповтора — что раньше.
        let mut wake = self.clock.next_frame;
        if let Some(next_repeat) = self.keys.next_repeat_at()
            && next_repeat < wake
        {
            wake = next_repeat;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

impl App {
    /// Выполняет тики симуляции, но не дольше бюджета времени на кадр.
    ///
    /// Бюджет спасает окно от зависания на большой скорости.
    fn run_ticks(&mut self, ticks: u32) {
        let deadline = Instant::now() + TICK_BUDGET;

        for _ in 0..ticks {
            self.jar.simulation_step();
            if Instant::now() >= deadline {
                break;
            }
        }
    }
}

// ===========================================================================
// Тесты
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------------------
    // Помощники
    // ----------------------------------------------------------------------

    /// Банка заданного размера с одним существом по геному.
    fn jar_with(genome: CreatureGenome, size: usize) -> Jar {
        let mut jar = Jar::with_seed(size, 7);
        jar.creatures[0] = Some(Creature::new(genome));
        jar
    }

    /// Банка с одним существом в нужной клетке.
    fn jar_with_at(genome: CreatureGenome, size: usize, x: usize, y: usize) -> Jar {
        let mut jar = Jar::with_seed(size, 7);
        jar.creatures[x + y * size] = Some(Creature::new(genome));
        jar
    }

    /// Геном из одного гена.
    fn genome_of(action: CreatureAction) -> CreatureGenome {
        CreatureGenome::from_line(vec![action])
    }

    /// Сумма минералов: в поле плюс в существах.
    fn total_minerals(jar: &Jar) -> usize {
        let in_field: usize = jar.minerals.iter().sum();
        let in_creatures: usize = jar
            .creatures
            .iter()
            .flatten()
            .map(|creature| creature.minerals)
            .sum();
        in_field + in_creatures
    }

    /// Проверяет базовые инварианты мира после серии тиков.
    fn assert_invariants(jar: &Jar, config: &Config) {
        let max_lifespan = config.lifetime_max_ticks.max(config.lifetime_min_ticks);
        for creature in jar.creatures.iter().flatten() {
            assert!(
                creature.lifespan >= config.lifetime_min_ticks && creature.lifespan <= max_lifespan,
                "срок жизни {} вне диапазона {}..={}",
                creature.lifespan,
                config.lifetime_min_ticks,
                max_lifespan
            );
            assert!(
                creature.age <= creature.lifespan,
                "возраст {} превысил срок жизни {}",
                creature.age,
                creature.lifespan
            );
            assert!(
                !creature.genome.is_empty(),
                "геном не должен оставаться пустым"
            );
        }
    }

    // ----------------------------------------------------------------------
    // Симуляция: инварианты
    // ----------------------------------------------------------------------

    #[test]
    fn minerals_are_conserved_over_many_ticks() {
        for seed in 0..12u64 {
            let mut jar = Jar::with_seed(30, seed * 7919 + 1);
            spawn(&mut jar, &genome_top(), &[(10, 2), (11, 2)]);
            spawn(&mut jar, &genome_left(), &[(2, 10)]);

            let before = total_minerals(&jar);
            for _ in 0..400 {
                jar.simulation_step();
            }
            let after = total_minerals(&jar);

            assert_eq!(before, after, "сумма минералов изменилась на зерне {seed}");
        }
    }

    #[test]
    fn energy_never_goes_negative() {
        let mut jar = Jar::with_seed(20, 99);
        spawn(&mut jar, &genome_bottom(), &[(5, 5), (6, 5)]);

        for _ in 0..300 {
            jar.simulation_step();
        }

        // Энергия появляется только от фотосинтеза и делится:
        // ее количество ограничено суммой выращенного света.
        let total: usize = jar
            .creatures
            .iter()
            .flatten()
            .map(|creature| creature.energy)
            .sum();
        let grown = MAX_PHOTOSYNTHESIS_ENERGY * 300 * 2;
        assert!(
            total <= grown,
            "энергии больше, чем могло накопиться: {total}"
        );
    }

    #[test]
    fn creatures_die_at_the_end_of_their_lifetime() {
        let config = Config {
            lifetime_min_ticks: 5,
            lifetime_max_ticks: 5,
            ..Config::default()
        };
        let mut jar = Jar::with_config_and_seed(10, config, 3);
        jar.creatures[0] = Some(Creature::new(genome_of(CreatureAction::PhotoGrow)));
        jar.creatures[0].as_mut().unwrap().lifespan = 5;

        for _ in 0..4 {
            jar.simulation_step();
            assert!(jar.creatures[0].is_some(), "клетка умерла раньше срока");
        }

        jar.simulation_step();
        assert!(
            jar.creatures[0].is_none(),
            "клетка прожила больше срока жизни"
        );
    }

    #[test]
    fn lifespan_is_random_within_the_configured_range() {
        let config = Config::default();
        let mut rng = Rng::new(17);
        let mut seen_min = usize::MAX;
        let mut seen_max = 0usize;

        for _ in 0..2000 {
            let lifespan = random_lifespan(&mut rng, &config);
            assert!(
                (CREATURE_LIFETIME_MIN_TICKS..=CREATURE_LIFETIME_MAX_TICKS).contains(&lifespan),
                "срок жизни {lifespan} вне диапазона 900..=1100"
            );
            seen_min = seen_min.min(lifespan);
            seen_max = seen_max.max(lifespan);
        }

        assert!(
            seen_max - seen_min > 100,
            "разброс сроков жизни слишком маленький: {seen_min}..={seen_max}"
        );
    }

    #[test]
    fn two_jars_with_the_same_seed_behave_identically() {
        let mut first = Jar::with_seed(25, 4242);
        let mut second = Jar::with_seed(25, 4242);

        spawn(&mut first, &genome_top(), &[(10, 2), (11, 2), (12, 2)]);
        spawn(&mut second, &genome_top(), &[(10, 2), (11, 2), (12, 2)]);

        for _ in 0..250 {
            first.simulation_step();
            second.simulation_step();
        }

        assert_eq!(first.tick, second.tick);
        for (index, (left, right)) in first
            .creatures
            .iter()
            .zip(second.creatures.iter())
            .enumerate()
        {
            match (left, right) {
                (None, None) => {}
                (Some(left), Some(right)) => {
                    assert_eq!(
                        left.energy, right.energy,
                        "энергия разошлась в клетке {index}"
                    );
                    assert_eq!(
                        left.minerals, right.minerals,
                        "минералы разошлись в клетке {index}"
                    );
                    assert_eq!(left.age, right.age, "возраст разошлся в клетке {index}");
                    assert_eq!(
                        left.genome.genes(),
                        right.genome.genes(),
                        "геномы разошлись в клетке {index}"
                    );
                }
                _ => panic!("клетка {index}: в одной банке есть существо, в другой нет"),
            }
        }
    }

    #[test]
    fn newborns_start_at_zero_age() {
        let config = Config {
            split_energy: 0,
            split_minerals: 0,
            ..Config::default()
        };
        let mut jar = Jar::with_config_and_seed(10, config, 11);
        jar.creatures[0] = Some(Creature::new(genome_of(CreatureAction::Split)));

        jar.simulation_step();

        // Мать осталась на месте, ребенок появился рядом.
        let mother = jar.creatures[0].as_ref().expect("мать пропала");
        assert_eq!(mother.age, 0, "мать должна начать с нулевого возраста");
        assert_eq!(mother.last_action, None);
        assert_eq!(mother.genome.pointer(), 0);

        let child = jar
            .creatures
            .iter()
            .enumerate()
            .find(|(index, cell)| *index != 0 && cell.is_some())
            .map(|(_, cell)| cell.as_ref().unwrap())
            .expect("ребенок не появился");
        assert_eq!(child.age, 0, "новорожденный должен иметь нулевой возраст");
        assert_eq!(child.last_action, None);
        assert_eq!(child.genome.pointer(), 0);
    }

    // ----------------------------------------------------------------------
    // Симуляция: действия по отдельности
    // ----------------------------------------------------------------------

    #[test]
    fn photosynthesis_gives_energy_near_the_edge() {
        let size = 100;

        // Клетка в самом углу получает максимум света.
        let mut edge = jar_with_at(genome_of(CreatureAction::PhotoGrow), size, 0, 0);
        edge.simulation_step();
        let edge_energy = edge.creatures[0].as_ref().unwrap().energy;
        assert_eq!(
            edge_energy, MAX_PHOTOSYNTHESIS_ENERGY,
            "у края должно быть максимальное освещение"
        );

        // Клетка в центре в темноте.
        let mut center = jar_with_at(genome_of(CreatureAction::PhotoGrow), size, 50, 50);
        center.simulation_step();
        let center_energy = center.creatures[50 + 50 * size].as_ref().unwrap().energy;
        assert_eq!(center_energy, 0, "в центре должно быть темно");
    }

    #[test]
    fn eating_needs_energy_and_minerals() {
        // Без энергии поесть нельзя.
        let mut jar = jar_with(genome_of(CreatureAction::EatMinerals), 10);
        jar.simulation_step();
        let creature = jar.creatures[0].as_ref().unwrap();
        assert_eq!(creature.energy, 0);
        assert_eq!(creature.minerals, 0);
        assert_eq!(jar.minerals[0], 100, "минералы в поле не тронуты");

        // С энергией поесть можно: минерал переезжает в существо.
        let mut jar = jar_with(genome_of(CreatureAction::EatMinerals), 10);
        jar.creatures[0].as_mut().unwrap().energy = 5;
        jar.simulation_step();
        let creature = jar.creatures[0].as_ref().unwrap();
        assert_eq!(creature.energy, 4, "еда стоит один энергии");
        assert_eq!(creature.minerals, 1, "минерал должен попасть в существо");
        assert_eq!(jar.minerals[0], 99, "в поле минералов стало меньше");
    }

    #[test]
    fn eating_without_minerals_in_the_field_does_nothing() {
        let mut jar = jar_with(genome_of(CreatureAction::EatMinerals), 10);
        jar.creatures[0].as_mut().unwrap().energy = 5;
        jar.minerals[0] = 0;

        jar.simulation_step();

        let creature = jar.creatures[0].as_ref().unwrap();
        assert_eq!(creature.energy, 5, "энергия не должна тратиться зря");
        assert_eq!(creature.minerals, 0);
    }

    #[test]
    fn split_needs_energy_and_minerals() {
        // Энергию не хватает: деления нет.
        let mut jar = jar_with(genome_of(CreatureAction::Split), 10);
        jar.creatures[0].as_mut().unwrap().energy = 99;
        jar.creatures[0].as_mut().unwrap().minerals = 10;
        jar.simulation_step();
        assert_eq!(
            jar.creatures.iter().filter(|cell| cell.is_some()).count(),
            1,
            "без 100 энергии делиться нельзя"
        );

        // Минералов не хватает: деления нет.
        let mut jar = jar_with(genome_of(CreatureAction::Split), 10);
        jar.creatures[0].as_mut().unwrap().energy = 100;
        jar.creatures[0].as_mut().unwrap().minerals = 9;
        jar.simulation_step();
        assert_eq!(
            jar.creatures.iter().filter(|cell| cell.is_some()).count(),
            1,
            "без 10 минералов делиться нельзя"
        );
    }

    #[test]
    fn split_shares_energy_and_minerals_between_children() {
        let mut jar = jar_with(genome_of(CreatureAction::Split), 10);
        {
            let creature = jar.creatures[0].as_mut().unwrap();
            creature.energy = 100;
            creature.minerals = 11;
        }

        jar.simulation_step();

        let alive: Vec<&Creature> = jar.creatures.iter().flatten().collect();
        assert_eq!(alive.len(), 2, "должно быть две клетки");

        let total_energy: usize = alive.iter().map(|creature| creature.energy).sum();
        assert!(
            total_energy <= 100,
            "дети не могут получить больше энергии, чем было у матери: {total_energy}"
        );
        assert_eq!(total_energy, 80, "каждый ребенок получает 40 процентов");

        let total_minerals: usize = alive.iter().map(|creature| creature.minerals).sum();
        assert_eq!(total_minerals, 11, "минералы делятся без потерь");

        // Неделимый остаток достается матери.
        let mother_minerals = alive
            .iter()
            .find(|creature| creature.minerals > 0)
            .map(|creature| creature.minerals)
            .unwrap_or(0);
        assert_eq!(
            mother_minerals, 6,
            "нечетный остаток минералов достается матери"
        );
    }

    #[test]
    fn split_at_the_corner_stays_inside_the_field() {
        let mut jar = jar_with_at(genome_of(CreatureAction::Split), 10, 0, 0);
        {
            let creature = jar.creatures[0].as_mut().unwrap();
            creature.energy = 100;
            creature.minerals = 20;
        }

        jar.simulation_step();

        // Клетка разделилась, и обе части остались внутри поля.
        let alive: Vec<usize> = jar
            .creatures
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.is_some())
            .map(|(index, _)| index)
            .collect();

        assert_eq!(alive.len(), 2, "после деления должно быть две клетки");
        for index in &alive {
            assert!(*index < jar.size * jar.size, "клетка вышла за пределы поля");
        }

        // Мать осталась на своем месте, ребенок поселился рядом.
        assert!(jar.creatures[0].is_some(), "мать должна остаться на месте");
        let child = alive.iter().copied().find(|index| *index != 0).unwrap();
        let (x, y) = Grid::new(jar.size).xy(child);
        assert!(
            (x == 1 && y == 0) || (x == 0 && y == 1),
            "ребенок появился не рядом с матерью: ({x}, {y})"
        );
    }

    #[test]
    fn split_in_a_dead_end_keeps_the_cell_alive() {
        let mut jar = Jar::with_seed(10, 5);
        // Замораживаем всех соседей: делиться некуда.
        for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let x = 5 + dx;
            let y = 5 + dy;
            if x >= 0 && y >= 0 && (x as usize) < 10 && (y as usize) < 10 {
                jar.creatures[x as usize + y as usize * 10] =
                    Some(Creature::new(genome_of(CreatureAction::PhotoGrow)));
            }
        }

        let index = 5 + 5 * 10;
        jar.creatures[index] = Some(Creature::new(genome_of(CreatureAction::Split)));
        jar.creatures[index].as_mut().unwrap().energy = 150;
        jar.creatures[index].as_mut().unwrap().minerals = 20;

        let before = total_minerals(&jar);
        jar.simulation_step();
        let after = total_minerals(&jar);

        let cell = jar.creatures[index]
            .as_ref()
            .expect("в тупике клетка должна остаться жива");
        assert_eq!(
            cell.energy, 100,
            "энергия зажатой клетки не должна копиться выше порога деления"
        );
        assert_eq!(cell.minerals, 20, "минералы должны остаться у клетки");
        assert_eq!(before, after, "минералы не должны пропадать");
    }

    #[test]
    fn blocked_cell_divides_once_space_frees() {
        let mut jar = Jar::with_seed(10, 5);
        for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let x = 5 + dx;
            let y = 5 + dy;
            if x >= 0 && y >= 0 && (x as usize) < 10 && (y as usize) < 10 {
                jar.creatures[x as usize + y as usize * 10] =
                    Some(Creature::new(genome_of(CreatureAction::PhotoGrow)));
            }
        }

        let index = 5 + 5 * 10;
        jar.creatures[index] = Some(Creature::new(genome_of(CreatureAction::Split)));
        jar.creatures[index].as_mut().unwrap().energy = 100;
        jar.creatures[index].as_mut().unwrap().minerals = 20;

        // Первый тик: делиться некуда, клетка остается.
        jar.simulation_step();
        assert!(jar.creatures[index].is_some());

        // Освобождаем соседа справа - клетка сразу делится.
        jar.creatures[6 + 5 * 10] = None;
        jar.simulation_step();

        assert!(
            jar.creatures[index].is_some(),
            "мать должна остаться на месте"
        );
        assert!(
            jar.creatures[6 + 5 * 10].is_some(),
            "клетка должна поделиться в освободившуюся клетку"
        );
    }

    #[test]
    fn dying_of_old_age_returns_minerals_to_the_field() {
        let config = Config {
            lifetime_min_ticks: 3,
            lifetime_max_ticks: 3,
            ..Config::default()
        };
        let mut jar = Jar::with_config_and_seed(10, config, 21);
        jar.creatures[0] = Some(Creature::new(genome_of(CreatureAction::PhotoGrow)));
        jar.creatures[0].as_mut().unwrap().lifespan = 3;
        jar.creatures[0].as_mut().unwrap().minerals = 9;

        let before = total_minerals(&jar);
        for _ in 0..3 {
            jar.simulation_step();
        }
        let after = total_minerals(&jar);

        assert!(
            jar.creatures[0].is_none(),
            "клетка должна умереть от старости"
        );
        assert_eq!(before, after, "минералы должны вернуться в поле");
    }

    #[test]
    fn jump_moves_the_dna_pointer() {
        let mut jar = jar_with(
            CreatureGenome::from_line(vec![
                CreatureAction::Jump(2),
                CreatureAction::PhotoGrow,
                CreatureAction::PhotoGrow,
            ]),
            10,
        );

        jar.simulation_step();

        // get_action дал +1, Jump(2) добавил еще +2.
        assert_eq!(jar.creatures[0].as_ref().unwrap().genome.pointer(), 3);
    }

    #[test]
    fn last_action_is_recorded_and_tick_grows() {
        let mut jar = jar_with(genome_of(CreatureAction::PhotoGrow), 10);
        assert_eq!(jar.tick, 0);

        jar.simulation_step();

        assert_eq!(jar.tick, 1, "тик должен увеличиваться");
        assert_eq!(
            jar.creatures[0].as_ref().unwrap().last_action,
            Some(CreatureAction::PhotoGrow),
            "действие должно записываться"
        );
    }

    // ----------------------------------------------------------------------
    // Геном
    // ----------------------------------------------------------------------

    #[test]
    fn empty_genome_is_rejected() {
        let result = std::panic::catch_unwind(|| {
            CreatureGenome::from_line(vec![]);
        });
        assert!(result.is_err(), "пустой геном должен отклоняться");
    }

    #[test]
    fn dna_pointer_wraps_around_the_genome() {
        let mut genome =
            CreatureGenome::from_line(vec![CreatureAction::PhotoGrow, CreatureAction::EatMinerals]);

        let first = genome.get_action();
        let second = genome.get_action();
        let third = genome.get_action();

        assert_eq!(first, CreatureAction::PhotoGrow);
        assert_eq!(second, CreatureAction::EatMinerals);
        assert_eq!(
            third,
            CreatureAction::PhotoGrow,
            "указатель должен оборачиваться"
        );
    }

    #[test]
    fn insert_mutation_makes_the_genome_longer() {
        let mut rng = Rng::new(5);
        let mut genome = CreatureGenome::standard();

        for _ in 0..50 {
            let before = genome.len();
            genome.apply_mutation(MutationKind::Insert, &mut rng);
            assert_eq!(genome.len(), before + 1, "вставка добавляет один ген");
        }
    }

    #[test]
    fn remove_mutation_makes_the_genome_shorter() {
        let mut rng = Rng::new(6);
        let mut genome = CreatureGenome::standard();

        for _ in 0..15 {
            let before = genome.len();
            genome.apply_mutation(MutationKind::Remove, &mut rng);
            assert_eq!(genome.len(), before - 1, "удаление убирает один ген");
        }
    }

    #[test]
    fn replace_mutation_keeps_the_genome_length() {
        let mut rng = Rng::new(7);
        let mut genome = CreatureGenome::standard();

        for _ in 0..50 {
            let before = genome.len();
            genome.apply_mutation(MutationKind::Replace, &mut rng);
            assert_eq!(genome.len(), before, "замена не меняет длину генома");
        }
    }

    #[test]
    fn value_mutation_changes_the_jump_value() {
        let mut rng = Rng::new(8);
        let mut genome = CreatureGenome::from_line(vec![CreatureAction::Jump(1)]);

        for _ in 0..50 {
            let before = genome.genes()[0];
            genome.apply_mutation(MutationKind::Value, &mut rng);
            assert_ne!(genome.genes()[0], before, "значение прыжка должно меняться");
            if let CreatureAction::Jump(value) = genome.genes()[0] {
                assert!(value != 0, "нулевой прыжок бессмысленен");
                assert!((-4..=4).contains(&value), "значение вне диапазона: {value}");
            }
        }
    }

    #[test]
    fn random_jump_delta_is_never_zero() {
        let mut rng = Rng::new(9);
        for _ in 0..1000 {
            let delta = random_jump_delta(&mut rng);
            assert_ne!(delta, 0, "дельта прыжка не должна быть нулевой");
            assert!((-4..=4).contains(&delta), "дельта вне диапазона: {delta}");
        }
    }

    #[test]
    fn random_action_produces_all_kinds() {
        let mut rng = Rng::new(10);
        let mut seen = [false; ActionKind::COUNT];

        for _ in 0..500 {
            seen[random_action(&mut rng).kind().index()] = true;
        }

        for (index, found) in seen.iter().enumerate() {
            assert!(
                *found,
                "тип гена {} ни разу не выпал",
                ActionKind::ALL[index].label()
            );
        }
    }

    #[test]
    fn chance_percent_has_the_expected_edges() {
        let mut rng = Rng::new(11);

        for _ in 0..200 {
            assert!(
                !rng.chance_percent(0),
                "шанс 0 процентов не должен срабатывать"
            );
            assert!(
                rng.chance_percent(100),
                "шанс 100 процентов должен срабатывать"
            );
        }
    }

    #[test]
    fn split_does_not_create_energy_out_of_thin_air() {
        let mut rng = Rng::new(12);
        let config = Config::default();

        let mut mother = Creature::new(CreatureGenome::standard());
        mother.energy = 100;
        mother.minerals = 20;

        let (first, second) = mother.split(&mut rng, &config);

        assert!(
            first.energy + second.energy <= mother.energy,
            "суммарная энергия детей превысила энергию матери"
        );
        assert_eq!(first.minerals + second.minerals, mother.minerals);
    }

    // ----------------------------------------------------------------------
    // Новые гены: Share, Attack, JumpIf
    // ----------------------------------------------------------------------

    /// Банка с двумя клетками рядом: левая стоит левее правой.
    ///
    /// Индексы возвращаются в том же порядке, в котором клетки стоят
    /// на поле: левая обходится первой.
    fn jar_with_pair(left: CreatureGenome, right: CreatureGenome) -> (Jar, usize, usize) {
        let size = 9;
        let mut jar = Jar::with_seed(size, 3);
        let left_index = 4 + 4 * size;
        let right_index = 5 + 4 * size;
        jar.creatures[left_index] = Some(Creature::new(left));
        jar.creatures[right_index] = Some(Creature::new(right));
        (jar, left_index, right_index)
    }

    /// Генотип воина: сначала проверка "рядом нет чужих", потом удар.
    ///
    /// Пока чужих нет, указатель перепрыгивает удар и воин ждет.
    /// Как только чужой появился рядом, удар случается.
    fn warrior_genome() -> CreatureGenome {
        CreatureGenome::from_line(vec![
            CreatureAction::JumpIf(KinCondition::NoFoe, 1),
            CreatureAction::Attack,
        ])
    }

    #[test]
    fn share_moves_energy_to_a_neighbour() {
        let (mut jar, left, right) = jar_with_pair(
            genome_of(CreatureAction::Share),
            genome_of(CreatureAction::Jump(1)),
        );

        jar.creatures[left].as_mut().unwrap().energy = 100;
        jar.creatures[right].as_mut().unwrap().energy = 0;

        jar.simulation_step();

        let giver = jar.creatures[left].as_ref().unwrap();
        let taker = jar.creatures[right].as_ref().unwrap();

        assert_eq!(giver.energy, 100 - SHARE_AMOUNT, "щедрый не отдал энергию");
        assert_eq!(taker.energy, SHARE_AMOUNT, "сосед не получил энергию");
    }

    #[test]
    fn share_without_neighbours_costs_nothing() {
        let mut jar = jar_with(genome_of(CreatureAction::Share), 9);
        jar.creatures[0].as_mut().unwrap().energy = 100;

        jar.simulation_step();

        assert_eq!(
            jar.creatures[0].as_ref().unwrap().energy,
            100,
            "одиночка отдал энергию в пустоту"
        );
    }

    #[test]
    fn attack_takes_more_than_it_costs() {
        let (mut jar, left, right) = jar_with_pair(
            genome_of(CreatureAction::Attack),
            genome_of(CreatureAction::Jump(1)),
        );

        jar.creatures[left].as_mut().unwrap().energy = 100;
        jar.creatures[right].as_mut().unwrap().energy = 100;

        jar.simulation_step();

        let attacker = jar.creatures[left].as_ref().unwrap();
        let victim = jar.creatures[right].as_ref().unwrap();

        assert_eq!(attacker.energy, 100 - ATTACK_COST, "атака стоила не столько, сколько должна");
        assert_eq!(victim.energy, 100 - ATTACK_DAMAGE, "жертва потеряла не столько, сколько должна");
        assert!(
            ATTACK_DAMAGE > ATTACK_COST,
            "урон должен быть больше цены атаки, иначе война не истощает"
        );
    }

    #[test]
    fn attack_kills_when_energy_reaches_zero() {
        let (mut jar, left, right) = jar_with_pair(
            genome_of(CreatureAction::Attack),
            genome_of(CreatureAction::Jump(1)),
        );

        jar.creatures[left].as_mut().unwrap().energy = 100;
        let victim = jar.creatures[right].as_mut().unwrap();
        victim.energy = ATTACK_DAMAGE;
        victim.minerals = 7;

        let minerals_before = jar.minerals[right];
        jar.simulation_step();

        assert!(jar.creatures[right].is_none(), "жертва выжила после смертельного удара");
        assert_eq!(
            jar.minerals[right],
            minerals_before + 7,
            "минералы погибшей клетки не вернулись в поле"
        );
    }

    #[test]
    fn attack_without_neighbours_costs_nothing() {
        let mut jar = jar_with(genome_of(CreatureAction::Attack), 9);
        jar.creatures[0].as_mut().unwrap().energy = 100;

        jar.simulation_step();

        assert_eq!(
            jar.creatures[0].as_ref().unwrap().energy,
            100,
            "атака в пустоту стоила энергии"
        );
    }

    #[test]
    fn jump_if_low_energy_shifts_the_pointer_only_when_hungry() {
        let genome = CreatureGenome::from_line(vec![
            CreatureAction::JumpIf(KinCondition::LowEnergy, 1),
            CreatureAction::PhotoGrow,
        ]);

        let mut hungry = jar_with(genome.clone(), 9);
        hungry.creatures[0].as_mut().unwrap().energy = LOW_ENERGY - 1;
        hungry.simulation_step();
        assert_eq!(
            hungry.creatures[0].as_ref().unwrap().genome.line_pointer,
            2,
            "голодная клетка должна сместить указатель"
        );

        let mut full = jar_with(genome, 9);
        full.creatures[0].as_mut().unwrap().energy = LOW_ENERGY;
        full.simulation_step();
        assert_eq!(
            full.creatures[0].as_ref().unwrap().genome.line_pointer,
            1,
            "сытая клетка не должна смещать указатель"
        );
    }

    #[test]
    fn jump_if_reads_the_genomes_of_the_neighbours() {
        let genome = CreatureGenome::from_line(vec![
            CreatureAction::JumpIf(KinCondition::HasFoe, 1),
            CreatureAction::PhotoGrow,
        ]);

        // Сосед с другой ДНК - чужой, поэтому указатель смещается.
        let (mut with_foe, _, _) = jar_with_pair(genome.clone(), genome_of(CreatureAction::Jump(1)));
        with_foe.simulation_step();
        assert_eq!(
            with_foe.creatures[40].as_ref().unwrap().genome.line_pointer,
            2,
            "клетка должна заметить чужую ДНК рядом"
        );

        // Сосед с такой же ДНК - свой, поэтому указатель на месте.
        let (mut with_kin, _, _) = jar_with_pair(genome.clone(), genome);
        with_kin.simulation_step();
        assert_eq!(
            with_kin.creatures[40].as_ref().unwrap().genome.line_pointer,
            1,
            "клетка не должна считать своего чужим"
        );
    }

    #[test]
    fn kin_similarity_is_symmetric_and_ignores_gene_order() {
        let first = CreatureGenome::from_line(vec![
            CreatureAction::PhotoGrow,
            CreatureAction::EatMinerals,
            CreatureAction::Split,
            CreatureAction::PhotoGrow,
        ]);
        let second = CreatureGenome::from_line(vec![
            CreatureAction::Split,
            CreatureAction::PhotoGrow,
            CreatureAction::PhotoGrow,
            CreatureAction::EatMinerals,
        ]);

        assert_eq!(
            first.similarity_to(&second),
            100,
            "порядок генов не должен влиять на сходство"
        );
        assert_eq!(
            second.similarity_to(&first),
            first.similarity_to(&second),
            "сходство должно быть симметричным"
        );
    }

    #[test]
    fn kin_similarity_falls_after_enough_replacements() {
        let mut rng = Rng::new(21);
        let ancestor = CreatureGenome::standard();
        let mut genome = ancestor.clone();

        for _ in 0..200 {
            genome.apply_mutation(MutationKind::Replace, &mut rng);
        }

        assert!(
            genome.similarity_to(&ancestor) < KIN_SIMILARITY_PERCENT,
            "после многих замен клетка должна стать чужой: {}%",
            genome.similarity_to(&ancestor)
        );
    }

    #[test]
    fn start_genomes_are_kins_of_each_other() {
        // Стартовые геномы отличаются всего на один ген, поэтому все
        // четыре колонии начинают как одна семья: сначала они сливаются,
        // а расходятся уже в ходе эволюции.
        let genomes = [genome_top(), genome_right(), genome_bottom(), genome_left()];

        for i in 0..genomes.len() {
            for j in (i + 1)..genomes.len() {
                let similarity = genomes[i].similarity_to(&genomes[j]);
                assert!(
                    similarity >= KIN_SIMILARITY_PERCENT,
                    "стартовые геномы {i} и {j} слишком разные: {similarity}%"
                );
            }
        }
    }

    #[test]
    fn start_genomes_differ_from_the_base_only_by_one_gene() {
        let base = genome_base();

        for genome in [genome_top(), genome_right(), genome_bottom(), genome_left()] {
            assert_eq!(
                genome.len(),
                base.len(),
                "геномы должны отличаться от базы только составом, а не длиной"
            );
        }
    }

    #[test]
    fn war_starts_between_different_genomes() {
        let (mut jar, left, right) = jar_with_pair(warrior_genome(), genome_of(CreatureAction::Jump(1)));

        jar.creatures[left].as_mut().unwrap().energy = 100;
        jar.creatures[right].as_mut().unwrap().energy = 100;

        // Первый тик: воин только замечает чужого рядом.
        jar.simulation_step();
        assert_eq!(
            jar.creatures[left].as_ref().unwrap().energy,
            100,
            "воин ударил в первый же тик"
        );

        // Второй тик: удар.
        jar.simulation_step();

        let attacker = jar.creatures[left].as_ref().unwrap();
        let victim = jar.creatures[right].as_ref().unwrap();
        assert_eq!(attacker.energy, 100 - ATTACK_COST, "воин не заплатил за удар");
        assert_eq!(victim.energy, 100 - ATTACK_DAMAGE, "чужая клетка не пострадала");
    }

    #[test]
    fn kin_do_not_fight_each_other() {
        let (mut jar, left, right) = jar_with_pair(warrior_genome(), warrior_genome());

        jar.creatures[left].as_mut().unwrap().energy = 100;
        jar.creatures[right].as_mut().unwrap().energy = 100;

        for _ in 0..10 {
            jar.simulation_step();
        }

        assert_eq!(
            jar.creatures[left].as_ref().unwrap().energy,
            100,
            "свои подрались между собой"
        );
        assert_eq!(
            jar.creatures[right].as_ref().unwrap().energy,
            100,
            "свои подрались между собой"
        );
    }

    // ----------------------------------------------------------------------
    // Чистые функции
    // ----------------------------------------------------------------------

    #[test]
    fn light_level_is_maximum_at_the_edge() {
        let size = 100;
        assert_eq!(
            light_level(0, 0, size, MAX_PHOTOSYNTHESIS_ENERGY),
            MAX_PHOTOSYNTHESIS_ENERGY
        );
        assert_eq!(
            light_level(99, 50, size, MAX_PHOTOSYNTHESIS_ENERGY),
            MAX_PHOTOSYNTHESIS_ENERGY
        );
        assert_eq!(
            light_level(50, 0, size, MAX_PHOTOSYNTHESIS_ENERGY),
            MAX_PHOTOSYNTHESIS_ENERGY
        );
        assert_eq!(
            light_level(50, 99, size, MAX_PHOTOSYNTHESIS_ENERGY),
            MAX_PHOTOSYNTHESIS_ENERGY
        );
    }

    #[test]
    fn light_fades_towards_the_center_and_stops_at_reach() {
        let size = 100;
        let reach = light_reach(size);

        assert_eq!(
            light_level(reach, reach, size, MAX_PHOTOSYNTHESIS_ENERGY),
            0
        );
        assert_eq!(
            light_level(reach + 10, reach + 10, size, MAX_PHOTOSYNTHESIS_ENERGY),
            0
        );

        // Свет только убывает от края к центру.
        let mut previous = MAX_PHOTOSYNTHESIS_ENERGY + 1;
        for distance in 0..reach {
            let level = light_level(distance, 0, size, MAX_PHOTOSYNTHESIS_ENERGY);
            assert!(
                level <= previous,
                "освещенность растет с расстоянием: {distance}"
            );
            previous = level;
        }
    }

    #[test]
    fn light_level_works_on_tiny_fields() {
        for size in 1..=4 {
            for x in 0..size {
                for y in 0..size {
                    let level = light_level(x, y, size, MAX_PHOTOSYNTHESIS_ENERGY);
                    assert!(
                        level <= MAX_PHOTOSYNTHESIS_ENERGY,
                        "свет не может быть больше максимума"
                    );
                }
            }
        }
    }

    #[test]
    fn light_table_agrees_with_light_level() {
        let size = 100;
        let cell = 8;
        let reach = light_reach(size);
        let table = build_light_table(reach as u32 * cell, cell, size);

        // Обе шкалы считают свет по одной формуле, поэтому на большом
        // числе ступеней они совпадают: 255 уровней - это как раз шкала
        // таблицы освещенности.
        const SCALE: usize = 255;

        for distance in 0..reach {
            // Клетка на расстоянии `distance` от края: и по горизонтали,
            // и по вертикали.
            let level = light_level(distance, distance, size, SCALE);
            let from_table = table[distance * cell as usize] as usize;
            assert!(
                level.abs_diff(from_table) <= 1,
                "свет в клетке {distance} не совпал с таблицей: {level} против {from_table}"
            );
        }
    }

    #[test]
    fn light_level_is_quantized_by_the_energy_scale() {
        let size = 100;

        // Свет в клетке не может быть больше максимума энергии за тик.
        for distance in 0..light_reach(size) {
            let level = light_level(distance, 0, size, MAX_PHOTOSYNTHESIS_ENERGY);
            assert!(
                level <= MAX_PHOTOSYNTHESIS_ENERGY,
                "свет превысил максимум энергии: {level}"
            );
        }
    }

    #[test]
    fn speed_changes_in_both_directions() {
        assert_eq!(
            Speed::TicksPerFrame(1).slower(),
            Speed::FramesPerTick(2),
            "замедление переводит в режим один тик на два кадра"
        );
        assert_eq!(
            Speed::FramesPerTick(2).faster(),
            Speed::TicksPerFrame(1),
            "ускорение возвращает в режим тик за кадр"
        );
        assert_eq!(Speed::TicksPerFrame(3).faster(), Speed::TicksPerFrame(4));
        assert_eq!(Speed::TicksPerFrame(3).slower(), Speed::TicksPerFrame(2));
        assert_eq!(Speed::FramesPerTick(4).slower(), Speed::FramesPerTick(5));
    }

    #[test]
    fn speed_saturates_at_the_limits() {
        assert_eq!(
            Speed::FramesPerTick(MAX_FRAMES_PER_TICK).slower(),
            Speed::FramesPerTick(MAX_FRAMES_PER_TICK),
            "замедление упирается в потолок"
        );
        assert_eq!(
            Speed::TicksPerFrame(MAX_SPEED).faster(),
            Speed::TicksPerFrame(MAX_SPEED),
            "ускорение упирается в потолок"
        );
    }

    #[test]
    fn view_mode_cycles_through_all_modes() {
        let mut mode = ViewMode::Creatures;
        let mut seen = Vec::new();

        for _ in 0..ViewMode::ALL.len() {
            assert!(!seen.contains(&mode), "режим {mode:?} повторился");
            seen.push(mode);
            mode = mode.next();
        }

        assert_eq!(
            mode,
            ViewMode::Creatures,
            "после полного круга должен снова быть первый режим"
        );
        assert_eq!(seen.len(), ViewMode::ALL.len());
    }

    #[test]
    fn view_mode_labels_are_unique() {
        let mut labels = Vec::new();
        for mode in ViewMode::ALL {
            assert!(!labels.contains(&mode.label()), "метка повторилась");
            labels.push(mode.label());
        }
    }

    #[test]
    fn format_count_switches_units_at_the_right_boundaries() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(9_999), "9999");
        assert_eq!(format_count(10_000), "10.0k");
        assert_eq!(format_count(42_000), "42.0k");
        assert_eq!(format_count(999_999), "1.0m");
        assert_eq!(format_count(1_000_000), "1.0m");
        assert_eq!(format_count(2_500_000), "2.5m");
    }

    #[test]
    fn scale_and_mul255_match_the_exact_formula() {
        for value in 0..=255u32 {
            for factor in 0..=255u32 {
                let expected = (value * factor + 127) / 255;

                let scaled = mul255(value, factor);
                assert!(
                    scaled.abs_diff(expected) <= 1,
                    "mul255({value}, {factor}) = {scaled}, ожидалось {expected}"
                );

                let color = rgb(value as u8, value as u8, value as u8);
                let scaled_color = scale(color, factor);
                let channel = (scaled_color >> 16) & 0xFF;
                assert!(
                    channel.abs_diff(expected) <= 1,
                    "scale({value}, {factor}) дал {channel}, ожидалось {expected}"
                );
            }
        }
    }

    #[test]
    fn ramp_hits_its_stop_points() {
        for stop in MINERAL_RAMP.iter() {
            let (value, color) = *stop;
            assert_eq!(
                ramp(value, &MINERAL_RAMP),
                rgb(color.0 as u8, color.1 as u8, color.2 as u8),
                "минералы: стоп {value}"
            );
        }

        for stop in ENERGY_RAMP.iter() {
            let (value, color) = *stop;
            assert_eq!(
                ramp(value, &ENERGY_RAMP),
                rgb(color.0 as u8, color.1 as u8, color.2 as u8),
                "энергия: стоп {value}"
            );
        }

        assert_eq!(ramp(0, &MINERAL_RAMP), rgb(207, 227, 245));
        assert_eq!(ramp(255, &MINERAL_RAMP), rgb(18, 63, 126));
        assert_eq!(ramp(0, &ENERGY_RAMP), rgb(255, 224, 184));
        assert_eq!(ramp(255, &ENERGY_RAMP), rgb(122, 46, 8));
    }

    #[test]
    fn ramp_clamps_out_of_range_values() {
        assert_eq!(ramp(1000, &MINERAL_RAMP), ramp(255, &MINERAL_RAMP));
        assert_eq!(ramp(1000, &ENERGY_RAMP), ramp(255, &ENERGY_RAMP));
    }

    #[test]
    fn ramps_are_ordered_by_lightness() {
        for stops in [&MINERAL_RAMP[..], &ENERGY_RAMP[..]] {
            let mut previous = luma(ramp(0, stops));
            for t in 1..=255 {
                let current = luma(ramp(t, stops));
                assert!(
                    current <= previous,
                    "шкала не упорядочена по светлоте на значении {t}"
                );
                previous = current;
            }
        }
    }

    #[test]
    fn light_color_matches_the_ramp() {
        for intensity in 0..=255u8 {
            assert_eq!(
                light_color(intensity),
                ramp(intensity as u32, &LIGHT_RAMP),
                "цвета освещенности разошлись на значении {intensity}"
            );
        }
    }

    #[test]
    fn hsv_to_rgb_maps_the_primary_colors() {
        let red = hsv_to_rgb(0.0, 1.0, 1.0);
        assert_eq!(red >> 16, 0xFF, "нулевой тон должен быть красным");
        assert_eq!(red & 0xFF, 0, "нулевой тон не должен содержать синего");

        let green = hsv_to_rgb(120.0, 1.0, 1.0);
        assert_eq!(
            (green >> 8) & 0xFF,
            0xFF,
            "120 градусов должен быть зеленым"
        );
        assert_eq!(green >> 16, 0, "120 градусов не должен содержать красного");

        let blue = hsv_to_rgb(240.0, 1.0, 1.0);
        assert_eq!(blue & 0xFF, 0xFF, "240 градусов должен быть синим");
        assert_eq!(blue >> 16, 0, "240 градусов не должен содержать красного");
    }

    #[test]
    fn hsv_to_rgb_wraps_around_the_circle() {
        assert_eq!(hsv_to_rgb(360.0, 1.0, 1.0), hsv_to_rgb(0.0, 1.0, 1.0));
        assert_eq!(hsv_to_rgb(-120.0, 1.0, 1.0), hsv_to_rgb(240.0, 1.0, 1.0));
    }

    #[test]
    fn genome_mix_of_an_empty_genome_is_all_zero() {
        // Пустой геном создать нельзя, поэтому проверяем напрямую:
        // у настоящего генома доли всегда в сумме дают 255.
        let genome = CreatureGenome::standard();
        let mix = genome_mix(&genome);
        assert_eq!(mix.iter().map(|share| *share as u32).sum::<u32>(), 255);
    }

    #[test]
    fn genome_mix_does_not_invent_a_jump_share() {
        // В геноме без прыжков доля прыжков должна остаться нулевой.
        let genome = CreatureGenome::from_line(vec![
            CreatureAction::PhotoGrow,
            CreatureAction::PhotoGrow,
            CreatureAction::EatMinerals,
        ]);
        let mix = genome_mix(&genome);

        assert_eq!(
            mix[ActionKind::Jump.index()],
            0,
            "прыжков в геноме нет, но доля появилась"
        );
        assert_eq!(mix.iter().map(|share| *share as u32).sum::<u32>(), 255);
    }

    #[test]
    fn mix_label_and_action_label_cover_all_variants() {
        assert_eq!(action_label(None), "-");
        for kind in ActionKind::ALL {
            let action = match kind {
                ActionKind::PhotoGrow => CreatureAction::PhotoGrow,
                ActionKind::EatMinerals => CreatureAction::EatMinerals,
                ActionKind::Split => CreatureAction::Split,
                ActionKind::Jump => CreatureAction::Jump(1),
                ActionKind::Share => CreatureAction::Share,
                ActionKind::Attack => CreatureAction::Attack,
                ActionKind::JumpIf => CreatureAction::JumpIf(KinCondition::HasKin, 1),
            };
            assert_eq!(action_label(Some(action)), kind.label());
        }

        assert_eq!(mix_label(None), "-");
        let label = mix_label(Some([255, 0, 0, 0, 0, 0, 0]));
        assert!(label.contains("photo 100%"), "подпись состава: {label}");
        assert!(label.contains("jump 0%"), "подпись состава: {label}");
    }

    // ----------------------------------------------------------------------
    // Интерфейс: ввод и камера
    // ----------------------------------------------------------------------

    #[test]
    fn releasing_one_key_keeps_the_other_pressed() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(64);

        app.press_direction(Direction::Right);
        app.press_direction(Direction::Down);
        app.release_direction(Direction::Right);

        assert!(
            app.keys.is_held(),
            "после отпускания Right клавиша Down должна остаться нажатой"
        );
    }

    #[test]
    fn releasing_the_last_key_stops_the_repeat() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(64);

        app.press_direction(Direction::Right);
        app.release_direction(Direction::Right);

        assert!(!app.keys.is_held(), "автоповтор должен остановиться");
    }

    #[test]
    fn zoom_in_and_out_returns_to_the_same_size() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);

        let before = app.camera.cell_size;
        app.zoom(1);
        assert_ne!(app.camera.cell_size, before, "зум должен измениться");
        app.zoom(-1);
        assert_eq!(
            app.camera.cell_size, before,
            "шаг туда и шаг обратно должен вернуть тот же размер"
        );
    }

    #[test]
    fn zoom_saturates_at_the_limits() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);

        for _ in 0..50 {
            app.zoom(1);
        }
        assert_eq!(app.camera.cell_size, MAX_CELL_SIZE);

        for _ in 0..50 {
            app.zoom(-1);
        }
        assert_eq!(app.camera.cell_size, MIN_CELL_SIZE);
    }

    #[test]
    fn press_moves_once_and_repeat_takes_over_later() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(64);
        app.camera.cursor_x = 10;

        let now = Instant::now();
        app.press_direction(Direction::Right);
        assert_eq!(
            app.camera.cursor_x, 11,
            "нажатие должно сдвинуть на одну клетку"
        );
        assert!(app.keys.is_held());

        // До истечения задержки автоповтора ничего не происходит.
        assert_eq!(app.keys.poll(now), None);
        assert_eq!(app.camera.cursor_x, 11);

        // После задержки выделение едет само.
        assert_eq!(
            app.keys.poll(now + REPEAT_DELAY + Duration::from_millis(1)),
            Some(Direction::Right)
        );
    }

    #[test]
    fn command_is_recognized_by_key() {
        assert_eq!(
            command_from_key(PhysicalKey::Code(KeyCode::Space)),
            Some(Command::TogglePause)
        );
        assert_eq!(
            command_from_key(PhysicalKey::Code(KeyCode::Tab)),
            Some(Command::NextView)
        );
        assert_eq!(
            command_from_key(PhysicalKey::Code(KeyCode::KeyF)),
            Some(Command::FitZoom)
        );
        assert_eq!(
            command_from_key(PhysicalKey::Code(KeyCode::Equal)),
            Some(Command::SpeedUp)
        );
        assert_eq!(
            command_from_key(PhysicalKey::Code(KeyCode::Minus)),
            Some(Command::SpeedDown)
        );
        assert_eq!(command_from_key(PhysicalKey::Code(KeyCode::KeyW)), None);
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
    fn clock_runs_the_right_number_of_ticks() {
        let now = Instant::now();

        // Один тик за кадр.
        let mut clock = SimClock::new(now);
        assert_eq!(clock.ticks_due(now), 1);

        // Пауза: тиков нет.
        let mut clock = SimClock::new(now);
        clock.toggle_pause(now);
        assert_eq!(clock.ticks_due(now + FRAME_TIME * 10), 0);
        clock.toggle_pause(now + FRAME_TIME * 10);
        assert_eq!(clock.ticks_due(now + FRAME_TIME * 10), 1);

        // Несколько тиков за кадр.
        let mut clock = SimClock::new(now);
        clock.set_speed(Speed::TicksPerFrame(3));
        assert_eq!(clock.ticks_due(now), 3);

        // Один тик раз в четыре кадра.
        let mut clock = SimClock::new(now);
        clock.set_speed(Speed::FramesPerTick(4));
        let mut ticks = 0;
        for frame in 0..8 {
            ticks += clock.ticks_due(now + FRAME_TIME * frame);
        }
        assert_eq!(ticks, 2, "за восемь кадров должно быть два тика");
    }

    #[test]
    fn set_cell_size_clamps_and_moves_the_cursor_when_the_field_stops_fitting() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(8);
        assert!(
            !app.camera.selection_active(),
            "поле помещается, выделения быть не должно"
        );

        app.camera.set_cell_size(64);

        assert!(app.camera.selection_active());
        assert_eq!(
            (app.camera.cursor_x, app.camera.cursor_y),
            (10, 10),
            "выделение должно появиться в центре вида"
        );

        // Обратный переход курсор не двигает.
        let cursor = (app.camera.cursor_x, app.camera.cursor_y);
        app.camera.set_cell_size(8);
        assert_eq!(
            (app.camera.cursor_x, app.camera.cursor_y),
            cursor,
            "курсор не должен двигаться"
        );
    }

    #[test]
    fn flash_marks_cells_that_changed() {
        let jar = Jar::with_seed(20, 0xC0FFEE);

        let mut app = App::new(jar);
        app.update_flashes();
        assert!(
            app.flashes.iter().all(|flash| *flash == 0),
            "вспышки появились без изменений"
        );

        // Появилась новая клетка — на её месте вспышка.
        let index = 9 + 2 * 20;
        app.jar.creatures[index] = Some(Creature::new(genome_top()));
        app.refresh_snapshot();
        app.update_flashes();

        assert!(app.flashes[index] > 0, "рождение клетки не вызвало вспышку");
        assert!(
            app.flashes[8 + 2 * 20] == 0,
            "вспышка появилась на пустой клетке"
        );
    }

    #[test]
    fn flash_fades_out_after_a_few_frames() {
        let jar = Jar::with_seed(20, 0xC0FFEE);
        let mut app = App::new(jar);

        let index = 9 + 2 * 20;
        app.jar.creatures[index] = Some(Creature::new(genome_top()));
        app.refresh_snapshot();

        // Вспышка загорелась на полную силу.
        assert_eq!(
            app.flashes[index], FLASH_FRAMES,
            "вспышка должна начаться с полной силы"
        );

        // И гаснет по одному кадру.
        for frame in 1..=FLASH_FRAMES {
            app.update_flashes();
            assert_eq!(
                app.flashes[index],
                FLASH_FRAMES - frame,
                "вспышка должна гаснуть по кадру"
            );
        }
        assert_eq!(app.flashes[index], 0, "вспышка должна погаснуть");
    }

    #[test]
    fn title_text_shows_the_state_of_the_app() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);

        let title = app.title_text();
        assert!(title.contains("tick 0"), "заголовок: {title}");
        assert!(title.contains("pop 0"), "заголовок: {title}");
        assert!(title.contains("zoom fit"), "заголовок: {title}");
        assert!(!title.contains("paused"), "заголовок: {title}");

        app.toggle_pause();
        assert!(
            app.title_text().contains("paused"),
            "пауза должна быть видна"
        );

        app.toggle_pause();
        app.camera.set_cell_size(64);
        assert!(
            app.title_text().contains("zoom x"),
            "заголовок: {}",
            app.title_text()
        );
    }

    // ----------------------------------------------------------------------
    // Отрисовка
    // ----------------------------------------------------------------------

    /// Поле из одной клетки с одним существом.
    fn small_snapshot(size: usize) -> JarSnapshot {
        let mut jar = Jar::with_seed(size, 1);
        jar.creatures[0] = Some(Creature::new(genome_top()));
        jar.snapshot()
    }

    /// Пиксель буфера по координатам.
    fn pixel_at(buffer: &[u32], width: u32, x: i64, y: i64) -> u32 {
        buffer[(y * width as i64 + x) as usize]
    }

    /// Левый верхний угол поля в пикселях окна.
    ///
    /// Поле всегда по центру, пока помещается в окно целиком.
    fn field_origin(width: u32, height: u32, side: u32) -> (i64, i64) {
        (
            (width as i64 - side as i64) / 2,
            (height as i64 - side as i64) / 2,
        )
    }

    #[test]
    fn draw_frame_never_writes_outside_the_buffer() {
        for view in ViewMode::ALL {
            for cell_size in [3u32, 8, 64] {
                for (width, height) in [(320u32, 320u32), (800, 800), (137, 61)] {
                    let snapshot = small_snapshot(20);
                    let mut buffer = vec![0u32; (width * height) as usize];

                    draw_frame(
                        &mut buffer,
                        width,
                        height,
                        &snapshot,
                        view,
                        cell_size,
                        (0, 0),
                    );
                }
            }
        }
    }

    #[test]
    fn canvas_fill_rect_clips_to_the_window() {
        let mut buffer = vec![0u32; 10 * 10];

        // Полностью за пределами окна: ничего не меняется.
        {
            let mut canvas = Canvas {
                buffer: &mut buffer,
                width: 10,
                height: 10,
            };
            canvas.fill_rect(20, 20, 5, 5, 0x00FF_0000);
        }
        assert!(buffer.iter().all(|pixel| *pixel == 0));

        // Частично за пределами окна: закрашивается только видимая часть.
        {
            let mut canvas = Canvas {
                buffer: &mut buffer,
                width: 10,
                height: 10,
            };
            canvas.fill_rect(8, 8, 5, 5, 0x00FF_0000);
        }
        let changed = buffer.iter().filter(|pixel| **pixel == 0x00FF_0000).count();
        assert_eq!(changed, 4, "закрашено должно быть ровно 4 пикселя");

        // Отрицательные координаты: обрезаются по левому и верхнему краю.
        {
            let mut canvas = Canvas {
                buffer: &mut buffer,
                width: 10,
                height: 10,
            };
            canvas.fill_rect(-2, -2, 4, 4, 0x0000_FF00);
        }
        let changed = buffer.iter().filter(|pixel| **pixel == 0x0000_FF00).count();
        assert_eq!(changed, 4, "закрашено должно быть ровно 4 пикселя");

        // Нулевой размер: ничего не происходит.
        {
            let mut canvas = Canvas {
                buffer: &mut buffer,
                width: 10,
                height: 10,
            };
            canvas.fill_rect(0, 0, 0, 0, 0x0000_00FF);
        }
        assert!(!buffer.contains(&0x0000_00FF));
    }

    #[test]
    fn creature_cell_is_painted_in_the_creature_color() {
        let size = 5u32;
        let snapshot = small_snapshot(size as usize);
        let palette = Palette::new();

        let width = 400;
        let height = 400;
        let mut buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Creatures,
            8,
            (0, 0),
        );

        // Поле 5x5 клеток по 8 пикселей: смещение от края окна.
        let (offset_x, offset_y) = field_origin(width, height, size * 8);
        let pixel = pixel_at(&buffer, width, offset_x + 4, offset_y + 4);

        assert_eq!(
            pixel, palette.creature,
            "клетка с существом должна быть покрашена в цвет существа"
        );
    }

    #[test]
    fn empty_cell_is_painted_in_the_jar_background() {
        let size = 5u32;
        let snapshot = small_snapshot(size as usize);
        let palette = Palette::new();

        let width = 400;
        let height = 400;
        let mut buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Creatures,
            8,
            (0, 0),
        );

        let (offset_x, offset_y) = field_origin(width, height, size * 8);
        // Клетка (4, 4) пустая: под ней нет существа.
        let pixel = pixel_at(&buffer, width, offset_x + 4 * 8 + 4, offset_y + 4 * 8 + 4);

        assert_eq!(
            pixel, palette.jar_bg,
            "пустая клетка должна быть цвета фона банки"
        );
    }

    #[test]
    fn jar_frame_is_painted_around_the_field() {
        let size = 5u32;
        let snapshot = small_snapshot(size as usize);
        let palette = Palette::new();

        let width = 400;
        let height = 400;
        let mut buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Creatures,
            8,
            (0, 0),
        );

        let (offset_x, offset_y) = field_origin(width, height, size * 8);
        let pixel = pixel_at(&buffer, width, offset_x, offset_y);

        assert_eq!(
            pixel, palette.jar_frame,
            "угол рамки банки должен быть цвета рамки"
        );
    }

    #[test]
    fn colony_outline_draws_four_sides_for_a_lone_cell() {
        let mut jar = Jar::with_seed(5, 1);
        jar.creatures[2 + 2 * 5] = Some(Creature::new(genome_top()));
        let snapshot = jar.snapshot();

        let width = 400u32;
        let height = 400u32;
        let mut buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Creatures,
            8,
            (0, 0),
        );

        let (offset_x, offset_y) = field_origin(width, height, 5 * 8);
        let cell = 8i64;
        let x = offset_x + 2 * cell;
        let y = offset_y + 2 * cell;
        let frame = Palette::new().jar_frame;

        assert_eq!(pixel_at(&buffer, width, x, y), frame, "нет верхней стороны");
        assert_eq!(
            pixel_at(&buffer, width, x, y + cell - 1),
            frame,
            "нет нижней стороны"
        );
        assert_eq!(pixel_at(&buffer, width, x, y), frame, "нет левой стороны");
        assert_eq!(
            pixel_at(&buffer, width, x + cell - 1, y),
            frame,
            "нет правой стороны"
        );
    }

    #[test]
    fn colony_outline_has_no_line_between_neighbours() {
        let mut jar = Jar::with_seed(5, 1);
        jar.creatures[2 + 2 * 5] = Some(Creature::new(genome_top()));
        jar.creatures[3 + 2 * 5] = Some(Creature::new(genome_top()));
        let snapshot = jar.snapshot();

        let width = 400u32;
        let height = 400u32;
        let mut buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Creatures,
            8,
            (0, 0),
        );

        let (offset_x, offset_y) = field_origin(width, height, 5 * 8);
        let cell = 8i64;

        // Между соседними клетками линии быть не должно.
        // Проверяем середину общей границы, а не ее верхний край:
        // верхняя граница клетки всегда обведена.
        let between = offset_x + 3 * cell;
        let pixel = pixel_at(&buffer, width, between, offset_y + 2 * cell + cell / 2);
        assert_ne!(
            pixel,
            Palette::new().jar_frame,
            "между соседними клетками не должно быть линии"
        );
    }

    #[test]
    fn cursor_draws_corners_and_keeps_the_cell_centre() {
        let snapshot = small_snapshot(20);
        let palette = Palette::new();

        let width = 400u32;
        let height = 400u32;
        let mut buffer = vec![0u32; (width * height) as usize];
        // Размер клетки 32: поле 20x32 = 640 пикселей не влезает в окно,
        // значит выделение есть.
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Creatures,
            32,
            (0, 0),
        );

        let cell = 32i64;
        // Камера следует за курсором: он всегда в середине экрана.
        let (offset_x, offset_y) = field_offset((0, 0), 20, 32, width, height);

        // Уголки нарисованы.
        assert_ne!(
            pixel_at(&buffer, width, offset_x, offset_y),
            palette.jar_bg,
            "левый верхний уголок"
        );
        assert_ne!(
            pixel_at(&buffer, width, offset_x + cell - 1, offset_y),
            palette.jar_bg,
            "правый верхний уголок"
        );
        assert_ne!(
            pixel_at(&buffer, width, offset_x, offset_y + cell - 1),
            palette.jar_bg,
            "левый нижний уголок"
        );
        assert_ne!(
            pixel_at(&buffer, width, offset_x + cell - 1, offset_y + cell - 1),
            palette.jar_bg,
            "правый нижний уголок"
        );

        // Центр клетки не закрашен: там остается цвет самой клетки.
        let under = cell_color(&snapshot, 0, ViewMode::Creatures, &palette);
        assert_eq!(
            pixel_at(&buffer, width, offset_x + cell / 2, offset_y + cell / 2),
            under,
            "центр клетки должен остаться чистым"
        );
    }

    #[test]
    fn minimap_stays_inside_its_frame_for_any_field_size() {
        for size in [20usize, 100, 150] {
            let snapshot = small_snapshot(size);
            let palette = Palette::new();

            let width = 400u32;
            let height = 400u32;
            let mut buffer = vec![0u32; (width * height) as usize];
            draw_frame(
                &mut buffer,
                width,
                height,
                &snapshot,
                ViewMode::Creatures,
                4,
                (0, 0),
            );

            let x = (width - MINIMAP_SIZE - PANEL_MARGIN) as i64;
            let y = PANEL_MARGIN as i64;

            // Рамка мини-карты на месте.
            let border = pixel_at(&buffer, width, x - 1, y - 1);
            assert_eq!(border, palette.panel_border, "рамка мини-карты пропала");

            // Внутри рамки карта не выходит за ее пределы:
            // все точки существ лежат в квадрате MINIMAP_SIZE.
            for cell_x in 0..size {
                for cell_y in 0..size {
                    let (dx, dy) = minimap_position(cell_x, cell_y, size);
                    assert!(
                        dx < MINIMAP_SIZE as i64 && dy < MINIMAP_SIZE as i64,
                        "точка мини-карты вылезла за рамку: ({dx}, {dy})"
                    );
                }
            }
        }
    }

    #[test]
    fn panel_width_counts_characters_not_bytes() {
        let lines = ["é"];
        // Один символ — один глиф, значит ширина текста равна шагу шрифта.
        assert_eq!(panel_text_width(lines.iter().copied()), TEXT_ADVANCE);
        assert_eq!(panel_width_of(&lines), TEXT_ADVANCE + PANEL_PADDING * 2);
    }

    #[test]
    fn genome_bar_splits_the_cell_fully() {
        let mut jar = Jar::with_seed(5, 1);
        jar.creatures[0] = Some(Creature::new(genome_top()));
        let snapshot = jar.snapshot();

        let mix = snapshot.cells[0].genome_mix.expect("состав генома");
        let cell = 24u32;

        // Части полоски не могут быть шире клетки.
        let total: u32 = mix.iter().map(|share| (cell * *share as u32) / 255).sum();
        assert!(
            total <= cell,
            "части полоски не могут быть шире клетки: {total} против {cell}"
        );

        // Остаток добивается последним цветом, поэтому полоска
        // занимает клетку целиком.
        let rest = cell - total;
        assert_eq!(
            total + rest,
            cell,
            "полоска должна заполнять клетку целиком"
        );
    }

    #[test]
    fn genome_bar_is_not_drawn_on_small_cells() {
        let snapshot = small_snapshot(5);

        let width = 400u32;
        let height = 400u32;
        let mut buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Genome,
            8,
            (0, 0),
        );

        let palette = Palette::new();
        let (offset_x, offset_y) = field_origin(width, height, 5 * 8);

        // Нижний ряд клетки: полоски состава здесь быть не должно,
        // поэтому остается цвет генома.
        let genome_color = snapshot.cells[0].genome_color.expect("цвет генома");
        let pixel = pixel_at(&buffer, width, offset_x + 4, offset_y + 7);
        assert_eq!(
            pixel, genome_color,
            "на мелком масштабе полоска состава не рисуется"
        );

        // А на крупном масштабе полоска уже есть: цвет внизу клетки
        // отличается от цвета генома.
        let mut big_buffer = vec![0u32; (width * height) as usize];
        draw_frame(
            &mut big_buffer,
            width,
            height,
            &snapshot,
            ViewMode::Genome,
            24,
            (0, 0),
        );
        let (big_x, big_y) = field_origin(width, height, 5 * 24);
        let bar_pixel = pixel_at(&big_buffer, width, big_x + 4, big_y + 22);
        assert_ne!(
            bar_pixel, genome_color,
            "на крупном масштабе полоска состава рисуется"
        );
        assert!(
            palette.action_colors.contains(&bar_pixel),
            "полоска должна быть одного из цветов типов генов"
        );
    }

    #[test]
    fn cell_color_handles_degenerate_values() {
        let mut jar = Jar::with_seed(5, 1);
        jar.creatures[0] = Some(Creature::new(genome_top()));
        jar.creatures[0].as_mut().unwrap().energy = 500;
        let snapshot = jar.snapshot();
        let palette = Palette::new();

        // Пустое поле: максимум минералов равен нулю, но цвет считается.
        let empty = Jar::with_seed(5, 1).snapshot();
        let _ = cell_color(&empty, 0, ViewMode::Minerals, &palette);

        // Энергия больше ста: клетка просто светлее.
        let color = cell_color(&snapshot, 0, ViewMode::Creatures, &palette);
        let bright = cell_color(
            &{
                let mut jar = Jar::with_seed(5, 1);
                jar.creatures[0] = Some(Creature::new(genome_top()));
                jar.creatures[0].as_mut().unwrap().energy = 100;
                jar.snapshot()
            },
            0,
            ViewMode::Creatures,
            &palette,
        );
        assert_eq!(color, bright, "энергия выше ста не должна менять цвет");

        // Пустая клетка в режиме энергии: цвет фона банки.
        assert_eq!(
            cell_color(&snapshot, 1, ViewMode::Energy, &palette),
            palette.jar_bg
        );
    }

    // ----------------------------------------------------------------------
    // Общие проверки и нагрузочные тесты
    // ----------------------------------------------------------------------

    #[test]
    fn trails_settle_on_the_target_color() {
        // В режимах-тепкарт цвет клетки догоняет новые данные постепенно.
        let size = 5usize;
        let mut jar = Jar::with_seed(size, 1);
        jar.creatures[0] = Some(Creature::new(genome_top()));
        let snapshot = jar.snapshot();

        let width = 400u32;
        let height = 400u32;
        let mut buffer = vec![0u32; (width * height) as usize];
        let mut cell_layer = vec![Palette::new().jar_bg; size * size];

        let (offset_x, offset_y) = field_origin(width, height, size as u32 * 8);

        // Первый кадр: цвет клетки еще не догнал целевой.
        draw_frame_with_layer(
            &mut buffer,
            width,
            height,
            &snapshot,
            ViewMode::Minerals,
            8,
            (0, 0),
            &mut cell_layer,
        );
        let first = pixel_at(&buffer, width, offset_x + 4, offset_y + 4);

        // Через много кадров цвет приходит к целевому.
        // У этой клетки максимум минералов, значит целевой цвет -
        // самый темный стоп шкалы.
        let target = ramp(255, &MINERAL_RAMP);
        for _ in 0..60 {
            draw_frame_with_layer(
                &mut buffer,
                width,
                height,
                &snapshot,
                ViewMode::Minerals,
                8,
                (0, 0),
                &mut cell_layer,
            );
        }
        let last = pixel_at(&buffer, width, offset_x + 4, offset_y + 4);

        assert_ne!(first, target, "цвет должен сначала догонять");
        assert_eq!(last, target, "цвет должен в конце прийти к целевому");
    }

    #[test]
    fn cell_info_is_built_from_the_snapshot_alone() {
        let mut jar = Jar::with_seed(10, 5);
        jar.creatures[0] = Some(Creature::new(genome_of(CreatureAction::EatMinerals)));
        jar.creatures[0].as_mut().unwrap().energy = 5;
        jar.simulation_step();

        let snapshot = jar.snapshot();

        let alive = CellInfo::from_snapshot(&snapshot, 0, 0);
        assert!(alive.alive, "клетка с существом должна быть живой");
        assert_eq!(alive.energy, 4);
        assert_eq!(alive.eaten, 1, "инспектор должен видеть съеденные минералы");
        assert_eq!(alive.age, 1, "инспектор должен видеть возраст");
        assert!(alive.mix.is_some(), "состав генома должен быть виден");

        let empty = CellInfo::from_snapshot(&snapshot, 1, 0);
        assert!(!empty.alive, "пустая клетка не должна быть живой");
        assert_eq!(empty.energy, 0);
        assert_eq!(empty.eaten, 0);
        assert_eq!(empty.age, 0);
        assert!(empty.mix.is_none());
        assert_eq!(empty.minerals, snapshot.minerals[1]);
    }

    #[test]
    fn hud_and_inspector_lines_describe_the_state() {
        let snapshot = small_snapshot(5);
        let [top, bottom] = hud_lines(&snapshot, ViewMode::Minerals, "x2", "x2.0", true);

        assert!(top.contains("tick 0"), "строка HUD: {top}");
        assert!(top.contains("pop 1"), "строка HUD: {top}");
        assert!(bottom.contains("mode minerals"), "строка HUD: {bottom}");
        assert!(bottom.contains("speed x2"), "строка HUD: {bottom}");
        assert!(bottom.contains("PAUSED"), "строка HUD: {bottom}");

        let info = CellInfo::from_snapshot(&snapshot, 0, 0);
        let lines = inspector_lines(&info);
        assert_eq!(lines[0], "cell 0,0");
        assert!(lines[1].contains("energy"), "строки инспектора: {lines:?}");

        let empty = CellInfo::from_snapshot(&snapshot, 1, 0);
        let lines = inspector_lines(&empty);
        assert!(lines.contains(&"empty".to_string()), "строки: {lines:?}");
    }

    #[test]
    fn anchors_place_panels_in_the_right_corners() {
        let window = (800u32, 600u32);
        let size = (100u32, 40u32);

        assert_eq!(Anchor::TopLeft.place(window, size, 6), (6, 6));
        assert_eq!(Anchor::TopRight.place(window, size, 6), (694, 6));
        assert_eq!(Anchor::BottomLeft.place(window, size, 6), (6, 554));
        assert_eq!(Anchor::BottomRight.place(window, size, 6), (694, 554));
    }

    #[test]
    fn viewport_reports_only_visible_cells() {
        // Поле 100 клеток по 8 пикселей в окне 320x320: видно только часть.
        let viewport = Viewport {
            offset_x: 0,
            offset_y: 0,
            cell: 8,
            grid: 800,
            size: 100,
        };

        let (x0, x1, y0, y1) = viewport.visible_cells(320, 320);
        assert_eq!(x0, 0);
        assert_eq!(y0, 0);
        assert_eq!(x1, 40, "в окно влезает 40 клеток по 8 пикселей");
        assert_eq!(y1, 40);

        // Если поле сдвинуто, видны клетки с середины.
        let shifted = Viewport {
            offset_x: -400,
            offset_y: 0,
            cell: 8,
            grid: 800,
            size: 100,
        };
        let (x0, _, _, _) = shifted.visible_cells(320, 320);
        assert_eq!(x0, 50, "первая видимая клетка считается от края поля");
    }

    /// Хеш состояния мира: грубая, но устойчивая к перестановкам сводка.
    fn state_hash(jar: &Jar) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;

        let mut mix = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        };

        mix(jar.tick as u64);
        for (index, cell) in jar.creatures.iter().enumerate() {
            mix(index as u64);
            match cell {
                None => mix(0),
                Some(creature) => {
                    mix(1);
                    mix(creature.energy as u64);
                    mix(creature.minerals as u64);
                    mix(creature.age as u64);
                    for action in creature.genome.genes() {
                        match action {
                            CreatureAction::Split => mix(1),
                            CreatureAction::PhotoGrow => mix(2),
                            CreatureAction::EatMinerals => mix(3),
                            CreatureAction::Jump(delta) => mix(4 + *delta as u64),
                            CreatureAction::Share => mix(12),
                            CreatureAction::Attack => mix(13),
                            CreatureAction::JumpIf(condition, delta) => {
                                mix(14 + *condition as u64 * 8 + (*delta as i64 + 8) as u64)
                            }
                        }
                    }
                }
            }
        }
        for minerals in &jar.minerals {
            mix(*minerals as u64);
        }

        hash
    }

    #[test]
    fn fixed_seed_produces_a_known_state() {
        // Страховочный тест: любое случайное изменение правил симуляции
        // ломает его. Значение меняется только вместе с правилами -
        // тогда, когда изменение задумано.
        let mut jar = Jar::with_seed(30, 0xC0FFEE);
        spawn(&mut jar, &genome_top(), &[(10, 2), (11, 2), (12, 2)]);
        spawn(&mut jar, &genome_left(), &[(2, 10)]);

        for _ in 0..200 {
            jar.simulation_step();
        }

        assert_eq!(
            state_hash(&jar),
            7395578395058024994,
            "состояние симуляции изменилось: правила ведут себя иначе"
        );
    }

    #[test]
    fn standard_genome_has_16_genes() {
        assert_eq!(CreatureGenome::standard().len(), 16);
    }

    #[test]
    fn genome_line_is_not_reachable_from_outside() {
        // Гены живут внутри генома: изменить их можно только
        // через методы, и каждый пересчитывает цвет.
        let genome = CreatureGenome::standard();
        assert_eq!(genome.len(), genome.genes().len());
        assert_eq!(genome.color(), genome_color(&genome));
    }

    #[test]
    fn genome_color_is_cached_and_updated_after_mutation() {
        let genome = CreatureGenome::standard();
        let before = genome.color();

        let mut changed = genome.clone();
        changed.apply_mutation(MutationKind::Replace, &mut Rng::new(1));
        assert_eq!(changed.color(), genome_color(&changed));
        assert_eq!(
            changed.color(),
            compute_genome_color(changed.genes()),
            "цвет генома должен совпадать с расчетом по нити"
        );
        let _ = before;
    }

    #[test]
    fn snapshot_into_reuses_buffers_and_gives_the_same_result() {
        let mut jar = Jar::with_seed(10, 5);
        spawn(&mut jar, &genome_top(), &[(1, 1), (2, 2)]);
        jar.simulation_step();

        let fresh = jar.snapshot();

        // Тот же снимок, но через переиспользование буферов.
        let mut reused = JarSnapshot::new(10);
        jar.snapshot_into(&mut reused);
        jar.simulation_step();
        jar.snapshot_into(&mut reused);

        let again = jar.snapshot();
        assert_eq!(again.cells.len(), reused.cells.len());
        assert_eq!(again.population, reused.population);
        assert_eq!(again.minerals, reused.minerals);
        for (left, right) in again.cells.iter().zip(reused.cells.iter()) {
            assert_eq!(left.alive, right.alive);
            assert_eq!(left.energy, right.energy);
            assert_eq!(left.eaten, right.eaten);
            assert_eq!(left.age, right.age);
        }
        let _ = fresh;
    }

    #[test]
    fn snapshot_carries_eaten_minerals_and_age() {
        let mut jar = Jar::with_seed(10, 5);
        jar.creatures[0] = Some(Creature::new(genome_of(CreatureAction::EatMinerals)));
        jar.creatures[0].as_mut().unwrap().energy = 5;

        jar.simulation_step();

        let snapshot = jar.snapshot();
        assert_eq!(
            snapshot.cells[0].eaten, 1,
            "снимок должен помнить съеденные минералы"
        );
        assert_eq!(snapshot.cells[0].age, 1, "снимок должен помнить возраст");
        assert_eq!(snapshot.cells[1].eaten, 0);
    }

    #[test]
    fn jar_with_seed_is_reproducible() {
        let first = Jar::with_seed(10, 77);
        let second = Jar::with_seed(10, 77);

        assert_eq!(first.size, second.size);
        assert_eq!(
            first.config.lifetime_min_ticks,
            second.config.lifetime_min_ticks
        );
        assert_eq!(
            first.config.lifetime_max_ticks,
            second.config.lifetime_max_ticks
        );
        assert_eq!(first.minerals, second.minerals);
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
        spawn(&mut jar, &genome_top(), &[(9, 2), (10, 2), (11, 2)]);

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
    fn all_four_colonies_survive() {
        let mut jar = seeded_jar();
        jar.rng = Rng::new(0xC0FFEE);

        for _ in 0..300 {
            jar.simulation_step();
        }

        // Колония жива, если у неё остался хотя бы один потомок:
        // конкретная клетка может умереть от старости или задохнуться
        // в тупике, но линия должна продолжиться.
        for (name, genome) in [
            ("верх", genome_top()),
            ("право", genome_right()),
            ("низ", genome_bottom()),
            ("лево", genome_left()),
        ] {
            let alive = jar
                .creatures
                .iter()
                .flatten()
                .filter(|creature| {
                    creature.genome.similarity_to(&genome) >= KIN_SIMILARITY_PERCENT
                })
                .count();

            assert!(alive > 0, "колония {name} вымерла");
        }
    }

    #[test]
    fn initial_cells_spawn_in_random_places() {
        // Стартовые клетки не привязаны к краям: две банки с разными
        // зёрнами должны давать разные расстановки.
        let first = seeded_jar();
        let second = seeded_jar();

        let positions = |jar: &Jar| -> Vec<usize> {
            jar.creatures
                .iter()
                .enumerate()
                .filter(|(_, cell)| cell.is_some())
                .map(|(index, _)| index)
                .collect()
        };

        assert_eq!(positions(&first).len(), 12, "стартовых клеток должно быть двенадцать");
        assert_ne!(
            positions(&first),
            positions(&second),
            "стартовые клетки должны появляться в случайных местах"
        );
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
            assert!(!genome.is_empty());
        }
    }

    #[test]
    fn jump_value_mutation_changes_value() {
        let mut rng = Rng::new(123);
        let mut genome = CreatureGenome::from_line(vec![CreatureAction::Jump(5)]);

        assert!(genome.mutate_value(&mut rng));
        assert_ne!(genome.genes()[0], CreatureAction::Jump(5));
    }

    #[test]
    fn mutations_eventually_change_genome() {
        let mut rng = Rng::new(99);
        let mut genome = CreatureGenome::standard();
        let original = genome.genes().to_vec();

        for _ in 0..100 {
            genome.mutate(&mut rng);
            if genome.genes() != original.as_slice() {
                break;
            }
        }

        assert_ne!(genome.genes(), original.as_slice());
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
        let genome = CreatureGenome::standard();
        let before = genome_mix(&genome);

        let mutated = genome.with_genes(&[(0, CreatureAction::Jump(1))]);
        let after = genome_mix(&mutated);

        assert_ne!(before, after, "мутация не изменила состав генома");
        assert_eq!(after.iter().map(|share| *share as u32).sum::<u32>(), 255);
        assert!(
            after[ActionKind::Jump.index()] > before[ActionKind::Jump.index()],
            "доля прыжков не выросла"
        );
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
        let mut jar = Jar::with_seed(20, 0xC0FFEE);
        spawn(&mut jar, &genome_top(), &[(9, 2), (10, 2), (11, 2)]);

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
            snapshot.cells.iter().any(|cell| cell.action.is_some()),
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
        app.camera.set_window_size(800, 800);

        app.camera.set_cell_size(8);
        assert!(
            !app.camera.selection_active(),
            "поле помещается, выделения быть не должно"
        );

        app.camera.set_cell_size(64);
        assert!(
            app.camera.selection_active(),
            "поле не помещается, выделение нужно"
        );
    }

    #[test]
    fn selection_moves_by_exactly_one_cell() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(64);
        app.camera.cursor_x = 10;
        app.camera.cursor_y = 10;

        app.camera.move_selection(Direction::Right);
        assert_eq!((app.camera.cursor_x, app.camera.cursor_y), (11, 10));

        app.camera.move_selection(Direction::Up);
        assert_eq!((app.camera.cursor_x, app.camera.cursor_y), (11, 9));
    }

    #[test]
    fn selection_does_not_move_when_the_field_fits() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(8);
        app.camera.cursor_x = 10;
        app.camera.cursor_y = 10;

        app.camera.move_selection(Direction::Right);
        assert_eq!((app.camera.cursor_x, app.camera.cursor_y), (10, 10));
    }

    #[test]
    fn selection_stops_at_the_field_border() {
        let mut app = App::new(Jar::new(20));
        app.camera.set_window_size(800, 800);
        app.camera.set_cell_size(64);

        app.camera.cursor_x = 0;
        app.camera.cursor_y = 0;
        app.camera.move_selection(Direction::Left);
        app.camera.move_selection(Direction::Up);
        assert_eq!((app.camera.cursor_x, app.camera.cursor_y), (0, 0));

        app.camera.cursor_x = 19;
        app.camera.cursor_y = 19;
        app.camera.move_selection(Direction::Right);
        app.camera.move_selection(Direction::Down);
        assert_eq!((app.camera.cursor_x, app.camera.cursor_y), (19, 19));
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

    /// Нагрузочная проверка: много зерон, много тиков, инварианты целы.
    #[test]
    fn many_seeds_survive_many_ticks() {
        for seed in 0..50u64 {
            let config = Config {
                lifetime_min_ticks: 200,
                lifetime_max_ticks: 220,
                ..Config::default()
            };
            let mut jar = Jar::with_config_and_seed(30, config, seed * 104_729 + 3);
            spawn(&mut jar, &genome_top(), &[(10, 2), (11, 2)]);
            spawn(&mut jar, &genome_left(), &[(2, 10)]);

            let minerals = total_minerals(&jar);

            for _ in 0..2000 {
                jar.simulation_step();
            }

            assert_invariants(&jar, &config);
            assert_eq!(
                minerals,
                total_minerals(&jar),
                "минералы разошлись на зерне {seed}"
            );
        }
    }
}
