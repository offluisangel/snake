use std::{collections::VecDeque, time::Duration};

pub const BOARD_SIZE: i32 = 16;
const CELL_COUNT: usize = (BOARD_SIZE * BOARD_SIZE) as usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

impl From<(i32, i32)> for Cell {
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Ready,
    Running,
    Paused,
    Lost,
    Won,
}

impl Phase {
    pub fn can_select_difficulty(self) -> bool {
        matches!(self, Self::Ready | Self::Lost | Self::Won)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

pub struct Game {
    body: Vec<Cell>,
    previous_body: Vec<Cell>,
    food: Option<Cell>,
    last_meal: Option<Cell>,
    impact: Option<Cell>,
    direction: Direction,
    turns: VecDeque<Direction>,
    phase: Phase,
    difficulty: Difficulty,
    selected_difficulty: Difficulty,
    score: u32,
    random: u64,
    elapsed: Duration,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let mut game = Self {
            body: Vec::new(),
            previous_body: Vec::new(),
            food: None,
            last_meal: None,
            impact: None,
            direction: Direction::Right,
            turns: VecDeque::with_capacity(2),
            phase: Phase::Ready,
            difficulty: Difficulty::Normal,
            selected_difficulty: Difficulty::Normal,
            score: 0,
            random: seed.max(1),
            elapsed: Duration::ZERO,
        };
        game.reset();
        game
    }

    #[doc(hidden)]
    pub fn from_board(body: Vec<Cell>, direction: Direction, food: Cell, score: u32) -> Self {
        assert!(!body.is_empty() && body.len() < CELL_COUNT && !body.contains(&food));
        let mut game = Self::new(1);
        game.previous_body.clone_from(&body);
        game.body = body;
        game.direction = direction;
        game.food = Some(food);
        game.score = score;
        game.phase = Phase::Running;
        game
    }

    fn reset(&mut self) {
        let center = BOARD_SIZE / 2;
        self.body = [(center, center), (center - 1, center), (center - 2, center)]
            .map(Cell::from)
            .to_vec();
        self.previous_body.clone_from(&self.body);
        self.direction = Direction::Right;
        self.turns.clear();
        self.score = 0;
        self.last_meal = None;
        self.impact = None;
        self.elapsed = Duration::ZERO;
        self.place_food();
    }

    pub fn body(&self) -> &[Cell] {
        &self.body
    }

    pub fn previous_body(&self) -> &[Cell] {
        &self.previous_body
    }

    pub fn food(&self) -> Option<Cell> {
        self.food
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn score(&self) -> u32 {
        self.score
    }

    pub fn last_meal(&self) -> Option<Cell> {
        self.last_meal
    }

    pub fn impact(&self) -> Option<Cell> {
        self.impact
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    pub fn selected_difficulty(&self) -> Difficulty {
        self.selected_difficulty
    }

    pub fn select_difficulty(&mut self, difficulty: Difficulty) {
        if self.phase.can_select_difficulty() {
            self.selected_difficulty = difficulty;
        }
    }

    pub fn cells_per_second(&self) -> u32 {
        let base = match self.difficulty {
            Difficulty::Easy => 6,
            Difficulty::Normal => 8,
            Difficulty::Hard => 16,
        };
        base + self.score / 4
    }

    fn step_duration(&self) -> Duration {
        Duration::from_secs(1) / self.cells_per_second()
    }

    pub fn motion_progress(&self) -> f32 {
        if self.phase == Phase::Running {
            (self.elapsed.as_secs_f32() / self.step_duration().as_secs_f32()).min(1.0)
        } else {
            1.0
        }
    }

    pub fn toggle_play(&mut self) {
        match self.phase {
            Phase::Ready | Phase::Lost | Phase::Won => {
                if self.phase != Phase::Ready {
                    self.reset();
                }
                self.difficulty = self.selected_difficulty;
                self.phase = Phase::Running;
            }
            Phase::Paused => self.phase = Phase::Running,
            Phase::Running => self.pause(),
        }
    }

    pub fn pause(&mut self) {
        if self.phase == Phase::Running {
            self.phase = Phase::Paused;
            self.turns.clear();
            self.elapsed = Duration::ZERO;
            self.previous_body.clone_from(&self.body);
        }
    }

    pub fn turn(&mut self, direction: Direction, repeated: bool) -> bool {
        if repeated || self.phase != Phase::Running || self.turns.len() == 2 {
            return false;
        }
        let (old_x, old_y) = self.turns.back().copied().unwrap_or(self.direction).delta();
        let (new_x, new_y) = direction.delta();
        if old_x * new_x + old_y * new_y != 0 {
            return false;
        }
        self.turns.push_back(direction);
        true
    }

    pub fn advance(&mut self, elapsed: Duration) {
        if self.phase != Phase::Running {
            return;
        }
        self.elapsed += elapsed;
        while self.phase == Phase::Running && self.elapsed >= self.step_duration() {
            self.elapsed -= self.step_duration();
            self.step();
        }
    }

    pub fn step(&mut self) {
        if self.phase != Phase::Running {
            return;
        }
        let direction = self.turns.pop_front().unwrap_or(self.direction);
        let (dx, dy) = direction.delta();
        let head = self.body[0];
        let next = Cell {
            x: (head.x + dx).rem_euclid(BOARD_SIZE),
            y: (head.y + dy).rem_euclid(BOARD_SIZE),
        };
        let eating = self.food == Some(next);

        let occupied = self.body.len() - usize::from(!eating);
        if self.body[..occupied].contains(&next) {
            self.impact = Some(next);
            self.phase = Phase::Lost;
            return;
        }
        self.direction = direction;
        self.previous_body.clone_from(&self.body);
        if !eating {
            self.body.pop();
        }
        self.body.insert(0, next);
        self.last_meal = eating.then_some(next);
        if eating {
            self.score += 1;
            if self.body.len() == CELL_COUNT {
                self.food = None;
                self.phase = Phase::Won;
            } else {
                self.place_food();
            }
        }
    }

    fn place_food(&mut self) {
        let mut occupied = [false; CELL_COUNT];
        for cell in &self.body {
            occupied[(cell.y * BOARD_SIZE + cell.x) as usize] = true;
        }
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        let pick = (self.random % (CELL_COUNT - self.body.len()) as u64) as usize;
        self.food = occupied
            .iter()
            .enumerate()
            .filter(|&(_, occupied)| !occupied)
            .nth(pick)
            .map(|(index, _)| Cell::from((index as i32 % BOARD_SIZE, index as i32 / BOARD_SIZE)));
    }
}
