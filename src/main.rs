fn main() {
    let mut jar = Jar::new(5);
    let c0 = Creature { genome: CreatureGenome { line: vec![CreatureAction::PhotoGrow, CreatureAction::EatMinerals, CreatureAction::Split], line_pointer: 0 }, energy: 0, minerals: 0};
    jar.creatures[10 + (10 * 32)] = Some(c0);

    for _ in 0..1000 {
        jar.simulation_step();
        println!("Энергия материнской клетки: {}", jar.creatures[330].clone().unwrap().energy);
    }
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
    /// Размер квадратной "банки" как степень двойки.
    /// Размер 10 означает, что размер поля 2^10 x 2^10 = 1024x1024 
    pub fn new(log_size: usize) -> Self {
        let size = 2usize.pow(log_size as u32);
        let creatures = vec![None; size * size];
        let minerals = vec![100usize; size * size];

        Self { size, tick: 0, creatures, minerals }
    }

    pub fn simulation_step(&mut self) {
        self.tick += 1;
        for i in 0..(self.size * self.size) {
            if let Some(mut creature) = self.creatures[i].clone() { // Если в клетке есть существо, берем его копию
                match creature.genome.get_action() {
                    CreatureAction::Split => {
                        if creature.energy >= 100 && creature.minerals > 10 { // Только если ресурсов хватает
                            let (new_0, new_1) = creature.split(); // Деление ресурсов с приоритетом материнской клетке
                            self.creatures[i] = Some(new_0); // На месте материнской клетки

                            let i_x = i % self.size;
                            let i_y = i / self.size;

                            match self.tick % 4 { // Выбор места дочерней клетки
                                0 => { self.creatures[(i_x + 1) + (i_y)*self.size] = Some(new_1) },
                                1 => { self.creatures[(i_x - 1) + (i_y)*self.size] = Some(new_1) },
                                2 => { self.creatures[i_x + (i_y + 1)*self.size] = Some(new_1) },
                                3 => { self.creatures[i_x + (i_y - 1)*self.size] = Some(new_1) },
                                _ => unreachable!()
                            }
                        }
                        else {
                            self.creatures[i] = Some(creature);
                        }
                    },
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
                    },
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Creature {
    /// Пока что введем абстракцию, что ген 0 - это фотосинтезировать, а при достижении энергии 100 клетка делится в случайную сторону
    pub genome: CreatureGenome,
    
    pub energy: usize,
    pub minerals: usize
}
impl Creature {
    pub fn split(&self) -> (Self, Self) {
        let energy = self.energy;
        let minerals = self.minerals;

        let e_0 = (energy / 2) + (energy % 2);
        let e_1 = energy - e_0;

        let m_0 = (minerals / 2) + (minerals % 2);
        let m_1 = minerals - m_0;

        let mut c_0 = Creature {genome: self.genome.clone(), energy: e_0, minerals: m_0};
        let mut c_1 = Creature {genome: self.genome.clone(), energy: e_1, minerals: m_1};

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
    pub line_pointer: usize
}
impl CreatureGenome {
    /// Выдает текущее действие клетки, сдвигает указатель действия на следующий
    pub fn get_action(&mut self) -> CreatureAction {
        let now_pointer = self.line_pointer % self.line.len();
        self.line_pointer += 1;
        self.line[now_pointer]
    }
}