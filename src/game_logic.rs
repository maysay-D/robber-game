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
    pub fn new_filled(width: usize, height: usize) -> Self {
        let clamped_width = width.max(2);
        let clamped_height = height.max(2);
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
        self.is_adjacent(from, to) && self.stack_at(from) > 0 && self.stack_at(to) > 0
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
                    if self.stack_at(neighbor) > 0 {
                        return true;
                    }
                }
            }
        }
        false
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

#[cfg(test)]
mod tests {
    use super::{GameBoard, Position};

    #[test]
    fn move_stacks_on_destination() {
        let mut board = GameBoard::new_filled(3, 3);
        assert!(board.apply_move(Position { x: 0, y: 0 }, Position { x: 1, y: 0 }));
        assert_eq!(board.stack_at(Position { x: 0, y: 0 }), 0);
        assert_eq!(board.stack_at(Position { x: 1, y: 0 }), 2);
    }

    #[test]
    fn move_transfers_entire_stack() {
        let mut board = GameBoard::new_filled(3, 3);
        assert!(board.apply_move(Position { x: 0, y: 0 }, Position { x: 1, y: 0 }));
        assert!(board.apply_move(Position { x: 1, y: 0 }, Position { x: 2, y: 0 }));
        assert_eq!(board.stack_at(Position { x: 1, y: 0 }), 0);
        assert_eq!(board.stack_at(Position { x: 2, y: 0 }), 3);
    }

    #[test]
    fn move_requires_adjacent_occupied_destination() {
        let mut board = GameBoard::new_filled(2, 2);
        assert!(!board.apply_move(Position { x: 0, y: 0 }, Position { x: 1, y: 1 }));
        assert!(board.apply_move(Position { x: 0, y: 0 }, Position { x: 1, y: 0 }));
        assert!(!board.apply_move(Position { x: 0, y: 0 }, Position { x: 1, y: 0 }));
    }

    #[test]
    fn no_legal_move_when_single_stack_remains() {
        let mut board = GameBoard::new_filled(2, 2);
        assert!(board.apply_move(Position { x: 0, y: 0 }, Position { x: 1, y: 0 }));
        assert!(board.apply_move(Position { x: 0, y: 1 }, Position { x: 1, y: 1 }));
        assert!(board.apply_move(Position { x: 1, y: 1 }, Position { x: 1, y: 0 }));
        assert!(!board.has_legal_move());
    }

    #[test]
    fn rectangular_board_supports_move_and_bounds() {
        let mut board = GameBoard::new_filled(3, 4);
        assert!(board.apply_move(Position { x: 2, y: 3 }, Position { x: 2, y: 2 }));
        assert_eq!(board.stack_at(Position { x: 2, y: 3 }), 0);
        assert_eq!(board.stack_at(Position { x: 2, y: 2 }), 2);
        assert!(!board.can_move(Position { x: 2, y: 2 }, Position { x: 3, y: 2 }));
    }
}
