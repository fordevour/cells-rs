fn main() {
    let mut jar = Jar::new(8);
    let c0 = Creature { genome: vec![0], energy: 10, genome_pointer: 0};
    jar.grid[128 * 256 + 128] = Some(c0);

    for _ in 0..120 {
        jar.simulation_step();
        println!("{}", jar.grid[128*256+128].clone().unwrap().energy);
        println!("{:?}", jar.grid[128*256+129].clone().unwrap_or(Creature { genome: Vec::new(), genome_pointer: 0, energy: 0 }));
    }
}

pub struct Jar {
    /// Размер квадратной "банки" как степень двойки.
    /// Размер 10 означает, что размер поля 2^10 x 2^10 = 1024x1024
    pub log_size: usize,
    pub grid: Vec<Option<Creature>>,
}
impl Jar {
    pub fn new(log_size: usize) -> Self {
        Self { log_size, grid: vec![None; 2usize.pow(log_size as u32).pow(2)] }
    }

    pub fn simulation_step(&mut self) {
        for i in 0..self.grid.len() {
            if let Some(mut creature) = self.grid[i].clone() {
                if creature.energy >= 100 {

                    // Временное решение просто по 50 давать потомкам
                    let creature_new = Creature { genome: creature.genome.clone(), genome_pointer: 0, energy: 50};
                    creature.energy = 50;
                    
                    self.grid[i] = Some(creature);
                    self.grid[i + 1] = Some(creature_new);
                }
                // Если делишься - больше ничего другого не делаешь
                else {
                    let gene = creature.genome[creature.genome_pointer % creature.genome.len()];
                    match gene {
                        0 => creature.energy += 1,
                        _ => {}
                    }
                    self.grid[i] = Some(creature);
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Creature {
    /// Пока что введем абстракцию, что ген 0 - это фотосинтезировать, а при достижении энергии 100 клетка делится в случайную сторону
    pub genome: Vec<usize>,
    pub genome_pointer: usize,
    pub energy: usize,
}