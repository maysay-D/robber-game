#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
            Self::First => "先手",
            Self::Second => "後手",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CpuDifficulty {
    Weak,
    Medium,
    Strong,
}

impl CpuDifficulty {
    pub fn label(self) -> &'static str {
        match self {
            Self::Weak => "CPU（弱）",
            Self::Medium => "CPU（中）",
            Self::Strong => "CPU（強）",
        }
    }
}