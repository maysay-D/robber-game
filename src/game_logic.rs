#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

#[derive(Clone, Debug)]
pub struct GameBoard {
    width: usize,
    height: usize,
    cells: Vec<u32>,
}

impl GameBoard {
    pub(crate) fn set_stack_at(&mut self, p0: Position, p1: usize) {
        let idx = self.index(p0);
        self.cells[idx] = p1 as u32;
    }

    pub fn new_filled(width: usize, height: usize) -> Self {
        let clamped_width = width.max(1);
        let clamped_height = height.max(1);

        Self {
            width: clamped_width,
            height: clamped_height,
            cells: vec![1; clamped_width * clamped_height],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn stack_at(&self, pos: Position) -> u32 {
        self.cells[self.index(pos)]
    }

    pub fn can_move(&self, from: Position, to: Position) -> bool {
        if !self.in_bounds(from) || !self.in_bounds(to) || from == to {
            return false;
        }

        self.is_adjacent(from, to)
            && self.stack_at(from) > 0
            && self.stack_at(to) > 0
            && self.stack_at(from) >= self.stack_at(to)
    }

    pub fn apply_move(&mut self, from: Position, to: Position) -> bool {
        if !self.can_move(from, to) {
            return false;
        }

        let from_idx = self.index(from);
        let to_idx = self.index(to);
        let moving = self.cells[from_idx];

        self.cells[from_idx] = 0;
        self.cells[to_idx] += moving;

        true
    }

    pub fn has_legal_move(&self) -> bool {
        for y in 0..self.height {
            for x in 0..self.width {
                let pos = Position { x, y };

                if self.stack_at(pos) == 0 {
                    continue;
                }

                for neighbor in self.neighbors(pos) {
                    if self.can_move(pos, neighbor) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn legal_moves(&self) -> Vec<(Position, Position)> {
        let mut moves = Vec::new();

        for y in 0..self.height {
            for x in 0..self.width {
                let from = Position { x, y };

                if self.stack_at(from) == 0 {
                    continue;
                }

                for to in self.neighbors(from) {
                    if self.can_move(from, to) {
                        moves.push((from, to));
                    }
                }
            }
        }

        moves
    }

    fn index(&self, pos: Position) -> usize {
        pos.y * self.width + pos.x
    }

    fn in_bounds(&self, pos: Position) -> bool {
        pos.x < self.width && pos.y < self.height
    }

    fn is_adjacent(&self, a: Position, b: Position) -> bool {
        a.x.abs_diff(b.x) + a.y.abs_diff(b.y) == 1
    }

    fn neighbors(&self, pos: Position) -> impl Iterator<Item = Position> {
        let mut out = Vec::with_capacity(4);

        if pos.x > 0 {
            out.push(Position {
                x: pos.x - 1,
                y: pos.y,
            });
        }

        if pos.x + 1 < self.width {
            out.push(Position {
                x: pos.x + 1,
                y: pos.y,
            });
        }

        if pos.y > 0 {
            out.push(Position {
                x: pos.x,
                y: pos.y - 1,
            });
        }

        if pos.y + 1 < self.height {
            out.push(Position {
                x: pos.x,
                y: pos.y + 1,
            });
        }

        out.into_iter()
    }
}