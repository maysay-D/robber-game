use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlayerTurn {
    First,
    Second,
}

impl PlayerTurn {
    pub fn next(self) -> Self {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::First => "First Player",
            Self::Second => "Second Player",
        }
    }

    pub fn tint(self) -> bevy::prelude::Color {
        match self {
            Self::First => bevy::prelude::Color::srgba(0.2, 0.5, 1.0, 0.14),
            Self::Second => bevy::prelude::Color::srgba(1.0, 0.45, 0.2, 0.14),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CpuDifficulty {
    Weak,
    Medium,
    Strong,
}

impl CpuDifficulty {
    pub fn label(self) -> &'static str {
        match self {
            Self::Weak => "弱いCPU",
            Self::Medium => "中程度のCPU",
            Self::Strong => "強いCPU",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GameMessage {
    WaitingForOpponent,
    MatchStarted {
        side: PlayerTurn,
        board_width: usize,
        board_height: usize,
    },
    Move {
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    },
    RematchDecision {
        continue_playing: bool,
    },
    OpponentDisconnected,
}
