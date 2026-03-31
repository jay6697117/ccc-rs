use crate::types::{
    CommandState, FindType, IndentDir, Operator, PersistentState,
};

/// The result of processing one keypress in normal mode.
#[derive(Debug, PartialEq)]
pub enum TransitionResult {
    /// State advanced; no action required yet.
    StateChanged(CommandState),
    /// A motion-only action should be executed (move cursor).
    ExecuteMotion { motion_key: char, count: u32 },
    /// An operator+motion action should be executed.
    ExecuteOperator {
        op: Operator,
        motion_key: char,
        count: u32,
        op_count: u32,
    },
    /// An operator applied to the current line (e.g. `dd`, `cc`, `yy`).
    ExecuteLineOp { op: Operator, count: u32 },
    /// Replace single char under cursor.
    ExecuteReplace { ch: char },
    /// Indent/dedent current line.
    ExecuteIndent { dir: IndentDir, count: u32 },
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Repeat last change.
    RepeatLastChange,
    /// Enter insert mode (optionally at specific position).
    EnterInsert,
    /// Unrecognised input — reset to Idle.
    Reset,
}

/// Process a keypress in Normal mode.
/// Returns the transition result and the updated CommandState.
/// Corresponds to TS `transition()` in src/vim/transitions.ts.
pub fn transition(
    state: &CommandState,
    key: char,
    _persistent: &PersistentState,
) -> (TransitionResult, CommandState) {
    match state {
        CommandState::Idle => from_idle(key),
        CommandState::Count { count } => from_count(*count, key),
        CommandState::Operator { op, count } => from_operator(*op, *count, key),
        CommandState::OperatorCount { op, count, op_count } => {
            from_operator_count(*op, *count, *op_count, key)
        }
        CommandState::OperatorFind { op, count, find_type } => {
            from_operator_find(*op, *count, *find_type, key)
        }
        CommandState::OperatorTextObj { op, count } => from_operator_text_obj(*op, *count, key),
        CommandState::Find { find_type, count } => from_find(*find_type, *count, key),
        CommandState::G { count } => from_g(*count, key),
        CommandState::OperatorG { op, count } => from_operator_g(*op, *count, key),
        CommandState::Replace => from_replace(key),
        CommandState::Indent { dir, count } => from_indent(*dir, *count, key),
    }
}

fn from_idle(key: char) -> (TransitionResult, CommandState) {
    match key {
        '1'..='9' => {
            let count = key.to_digit(10).unwrap();
            (TransitionResult::StateChanged(CommandState::Count { count }), CommandState::Count { count })
        }
        'd' => op_state(Operator::Delete, 1),
        'c' => op_state(Operator::Change, 1),
        'y' => op_state(Operator::Yank, 1),
        'f' | 'F' | 't' | 'T' => {
            let ft = FindType::from_char(key).unwrap();
            let next = CommandState::Find { find_type: ft, count: 1 };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'g' => {
            let next = CommandState::G { count: 1 };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'r' => (
            TransitionResult::StateChanged(CommandState::Replace),
            CommandState::Replace,
        ),
        '>' => {
            let next = CommandState::Indent { dir: IndentDir::In, count: 1 };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        '<' => {
            let next = CommandState::Indent { dir: IndentDir::Out, count: 1 };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'u' => (TransitionResult::Undo, CommandState::Idle),
        '.' => (TransitionResult::RepeatLastChange, CommandState::Idle),
        'i' | 'a' | 'I' | 'A' | 'o' | 'O' | 's' => {
            (TransitionResult::EnterInsert, CommandState::Idle)
        }
        // Motion keys — let caller handle movement
        'h' | 'l' | 'j' | 'k' | 'w' | 'b' | 'e' | '0' | '$' | '^'
        | 'x' | 'X' | 'J' | '~' => {
            (TransitionResult::ExecuteMotion { motion_key: key, count: 1 }, CommandState::Idle)
        }
        _ => (TransitionResult::Reset, CommandState::Idle),
    }
}

fn from_count(count: u32, key: char) -> (TransitionResult, CommandState) {
    match key {
        '0'..='9' => {
            let new_count = count.saturating_mul(10).saturating_add(key.to_digit(10).unwrap());
            let next = CommandState::Count { count: new_count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'd' => op_state_with_count(Operator::Delete, count),
        'c' => op_state_with_count(Operator::Change, count),
        'y' => op_state_with_count(Operator::Yank, count),
        'f' | 'F' | 't' | 'T' => {
            let ft = FindType::from_char(key).unwrap();
            let next = CommandState::Find { find_type: ft, count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'g' => {
            let next = CommandState::G { count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        _ => (
            TransitionResult::ExecuteMotion { motion_key: key, count },
            CommandState::Idle,
        ),
    }
}

fn from_operator(op: Operator, count: u32, key: char) -> (TransitionResult, CommandState) {
    match key {
        '0'..='9' => {
            let op_count = key.to_digit(10).unwrap();
            let next = CommandState::OperatorCount { op, count, op_count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'i' | 'a' => {
            let next = CommandState::OperatorTextObj { op, count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'f' | 'F' | 't' | 'T' => {
            let ft = FindType::from_char(key).unwrap();
            let next = CommandState::OperatorFind { op, count, find_type: ft };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        'g' => {
            let next = CommandState::OperatorG { op, count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        // Same key as operator → line op (dd, cc, yy)
        c if op_char(op) == c => (
            TransitionResult::ExecuteLineOp { op, count },
            CommandState::Idle,
        ),
        // Any other key → operator + motion
        _ => (
            TransitionResult::ExecuteOperator {
                op,
                motion_key: key,
                count,
                op_count: 1,
            },
            CommandState::Idle,
        ),
    }
}

fn from_operator_count(
    op: Operator,
    count: u32,
    op_count: u32,
    key: char,
) -> (TransitionResult, CommandState) {
    match key {
        '0'..='9' => {
            let new_op_count =
                op_count.saturating_mul(10).saturating_add(key.to_digit(10).unwrap());
            let next = CommandState::OperatorCount { op, count, op_count: new_op_count };
            (TransitionResult::StateChanged(next.clone()), next)
        }
        _ => (
            TransitionResult::ExecuteOperator {
                op,
                motion_key: key,
                count,
                op_count,
            },
            CommandState::Idle,
        ),
    }
}

fn from_operator_find(
    op: Operator,
    count: u32,
    find_type: FindType,
    _key: char,
) -> (TransitionResult, CommandState) {
    // Next key is the find character
    (
        TransitionResult::ExecuteOperator {
            op,
            // encode find as a composite motion key — callers decode this
            motion_key: find_type_char(find_type),
            count,
            op_count: 1,
        },
        CommandState::Idle,
    )
}

fn from_operator_text_obj(op: Operator, count: u32, key: char) -> (TransitionResult, CommandState) {
    // key is the text object character (w, s, p, b, etc.)
    (
        TransitionResult::ExecuteOperator {
            op,
            motion_key: key,
            count,
            op_count: 1,
        },
        CommandState::Idle,
    )
}

fn from_find(find_type: FindType, count: u32, _key: char) -> (TransitionResult, CommandState) {
    // key is the target character — emit as motion for callers
    (
        TransitionResult::ExecuteMotion { motion_key: find_type_char(find_type), count },
        CommandState::Idle,
    )
}

fn from_g(count: u32, key: char) -> (TransitionResult, CommandState) {
    match key {
        'g' => (
            TransitionResult::ExecuteMotion { motion_key: 'g', count },
            CommandState::Idle,
        ),
        _ => (TransitionResult::Reset, CommandState::Idle),
    }
}

fn from_operator_g(op: Operator, count: u32, key: char) -> (TransitionResult, CommandState) {
    match key {
        'g' => (
            TransitionResult::ExecuteOperator {
                op,
                motion_key: 'g',
                count,
                op_count: 1,
            },
            CommandState::Idle,
        ),
        _ => (TransitionResult::Reset, CommandState::Idle),
    }
}

fn from_replace(key: char) -> (TransitionResult, CommandState) {
    (TransitionResult::ExecuteReplace { ch: key }, CommandState::Idle)
}

fn from_indent(dir: IndentDir, count: u32, key: char) -> (TransitionResult, CommandState) {
    // Second `>` or `<` confirms the indent action
    let expected = match dir {
        IndentDir::In => '>',
        IndentDir::Out => '<',
    };
    if key == expected {
        (TransitionResult::ExecuteIndent { dir, count }, CommandState::Idle)
    } else {
        (TransitionResult::Reset, CommandState::Idle)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn op_state(op: Operator, count: u32) -> (TransitionResult, CommandState) {
    let next = CommandState::Operator { op, count };
    (TransitionResult::StateChanged(next.clone()), next)
}

fn op_state_with_count(op: Operator, count: u32) -> (TransitionResult, CommandState) {
    let next = CommandState::Operator { op, count };
    (TransitionResult::StateChanged(next.clone()), next)
}

fn op_char(op: Operator) -> char {
    match op {
        Operator::Delete => 'd',
        Operator::Change => 'c',
        Operator::Yank   => 'y',
    }
}

fn find_type_char(ft: FindType) -> char {
    match ft {
        FindType::ForwardInclusive  => 'f',
        FindType::BackwardInclusive => 'F',
        FindType::ForwardExclusive  => 't',
        FindType::BackwardExclusive => 'T',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PersistentState;

    fn idle() -> CommandState { CommandState::Idle }
    fn ps() -> PersistentState { PersistentState::default() }

    #[test]
    fn idle_d_enters_operator() {
        let (result, next) = transition(&idle(), 'd', &ps());
        assert_eq!(result, TransitionResult::StateChanged(CommandState::Operator { op: Operator::Delete, count: 1 }));
        assert_eq!(next, CommandState::Operator { op: Operator::Delete, count: 1 });
    }

    #[test]
    fn dd_is_line_op() {
        let state = CommandState::Operator { op: Operator::Delete, count: 1 };
        let (result, next) = transition(&state, 'd', &ps());
        assert_eq!(result, TransitionResult::ExecuteLineOp { op: Operator::Delete, count: 1 });
        assert_eq!(next, CommandState::Idle);
    }

    #[test]
    fn dw_is_operator_motion() {
        let state = CommandState::Operator { op: Operator::Delete, count: 1 };
        let (result, _) = transition(&state, 'w', &ps());
        assert_eq!(result, TransitionResult::ExecuteOperator {
            op: Operator::Delete,
            motion_key: 'w',
            count: 1,
            op_count: 1,
        });
    }

    #[test]
    fn count_then_d() {
        let (_, after_3) = transition(&idle(), '3', &ps());
        let (result, next) = transition(&after_3, 'd', &ps());
        assert_eq!(next, CommandState::Operator { op: Operator::Delete, count: 3 });
        let _ = result;
    }

    #[test]
    fn undo_from_idle() {
        let (result, next) = transition(&idle(), 'u', &ps());
        assert_eq!(result, TransitionResult::Undo);
        assert_eq!(next, CommandState::Idle);
    }

    #[test]
    fn replace_waits_for_char() {
        let (_, after_r) = transition(&idle(), 'r', &ps());
        let (result, next) = transition(&after_r, 'x', &ps());
        assert_eq!(result, TransitionResult::ExecuteReplace { ch: 'x' });
        assert_eq!(next, CommandState::Idle);
    }

    #[test]
    fn indent_confirmed_by_double_angle() {
        let (_, after_gt) = transition(&idle(), '>', &ps());
        let (result, _) = transition(&after_gt, '>', &ps());
        assert_eq!(result, TransitionResult::ExecuteIndent { dir: IndentDir::In, count: 1 });
    }

    #[test]
    fn gg_executes_motion() {
        let (_, after_g) = transition(&idle(), 'g', &ps());
        let (result, _) = transition(&after_g, 'g', &ps());
        assert_eq!(result, TransitionResult::ExecuteMotion { motion_key: 'g', count: 1 });
    }
}
