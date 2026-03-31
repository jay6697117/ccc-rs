/// Vim operator. Corresponds to TS `Operator` in src/vim/types.ts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
}

/// Find motion type. Corresponds to TS `FindType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindType {
    /// f — to char (inclusive)
    ForwardInclusive,
    /// F — to char backwards (inclusive)
    BackwardInclusive,
    /// t — till char (exclusive)
    ForwardExclusive,
    /// T — till char backwards (exclusive)
    BackwardExclusive,
}

impl FindType {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'f' => Some(Self::ForwardInclusive),
            'F' => Some(Self::BackwardInclusive),
            't' => Some(Self::ForwardExclusive),
            'T' => Some(Self::BackwardExclusive),
            _ => None,
        }
    }
    pub fn is_forward(self) -> bool {
        matches!(self, Self::ForwardInclusive | Self::ForwardExclusive)
    }
}

/// Text object scope. Corresponds to TS `TextObjScope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjScope {
    Inner,
    Around,
}

/// Normal-mode command state machine.
/// Corresponds to TS `CommandState` in src/vim/types.ts.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandState {
    Idle,
    Count       { count: u32 },
    Operator    { op: Operator, count: u32 },
    OperatorCount { op: Operator, count: u32, op_count: u32 },
    OperatorFind  { op: Operator, count: u32, find_type: FindType },
    OperatorTextObj { op: Operator, count: u32 },
    Find        { find_type: FindType, count: u32 },
    G           { count: u32 },
    OperatorG   { op: Operator, count: u32 },
    Replace,
    Indent      { dir: IndentDir, count: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentDir {
    In,   // >
    Out,  // <
}

/// Persistent state that survives mode transitions.
/// Corresponds to TS `PersistentState` in src/vim/types.ts.
#[derive(Debug, Clone, Default)]
pub struct PersistentState {
    pub last_change: Option<RecordedChange>,
    pub last_find: Option<LastFind>,
    pub register: String,
    pub register_is_linewise: bool,
}

/// A recorded change for dot-repeat (`.` command).
#[derive(Debug, Clone)]
pub struct RecordedChange {
    pub inserted_text: String,
}

/// Last find for `;` / `,` repeat.
#[derive(Debug, Clone, Copy)]
pub struct LastFind {
    pub find_type: FindType,
    pub ch: char,
}

/// Complete vim state. Corresponds to TS `VimState`.
#[derive(Debug, Clone)]
pub enum VimState {
    Insert { inserted_text: String },
    Normal { command: CommandState },
}

impl Default for VimState {
    fn default() -> Self {
        Self::Normal { command: CommandState::Idle }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_type_from_char() {
        assert_eq!(FindType::from_char('f'), Some(FindType::ForwardInclusive));
        assert_eq!(FindType::from_char('T'), Some(FindType::BackwardExclusive));
        assert_eq!(FindType::from_char('x'), None);
    }

    #[test]
    fn find_type_direction() {
        assert!(FindType::ForwardInclusive.is_forward());
        assert!(!FindType::BackwardInclusive.is_forward());
    }

    #[test]
    fn vim_state_default_is_normal_idle() {
        match VimState::default() {
            VimState::Normal { command: CommandState::Idle } => {},
            other => panic!("expected Normal/Idle, got {other:?}"),
        }
    }
}
