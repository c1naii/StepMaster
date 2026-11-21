//! "Stop" buttons: one cancel flag per long operation.
//! An operation calls `begin` when it starts (clears an old request) and checks `cancelled`
//! between steps; `op_cancel` from the UI raises the flag.
use crate::util::{self, tr, Res};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Diag,
    Cleanup,
    Speed,
    Dupes,
    Defender,
    Repair,
}

static FLAGS: [AtomicBool; 6] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

/// Diagnostics is in its Defender step right now: only then does its "Stop" cancel Defender
/// (otherwise it would cancel a scan the user started elsewhere).
pub static DIAG_IN_DEFENDER: AtomicBool = AtomicBool::new(false);

pub fn flag(op: Op) -> &'static AtomicBool {
    &FLAGS[op as usize]
}

/// Starts an operation: forgets a stop request left from an earlier run.
pub fn begin(op: Op) {
    flag(op).store(false, Ordering::SeqCst);
}

static RUNNING: [AtomicBool; 6] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

/// While alive, the operation counts as running; dropped when the command returns.
pub struct Running(Op);
impl Drop for Running {
    fn drop(&mut self) {
        RUNNING[self.0 as usize].store(false, Ordering::SeqCst);
    }
}

/// Like `begin`, but refuses a second copy of the same operation (e.g. the page was left and
/// opened again mid-run): it would share the Stop flag and clear the first run's Stop.
pub fn start(op: Op) -> Res<Running> {
    if RUNNING[op as usize].swap(true, Ordering::SeqCst) {
        return Err(tr("Уже выполняется — дождитесь окончания", "Already running — wait until it finishes"));
    }
    begin(op);
    Ok(Running(op))
}

pub fn cancelled(op: Op) -> bool {
    flag(op).load(Ordering::SeqCst)
}

/// Error returned by a stopped operation (the UI shows it as a calm note, not as a failure).
pub fn stopped() -> String {
    tr("Остановлено", "Stopped")
}

fn parse(op: &str) -> Option<Op> {
    Some(match op {
        "diag" => Op::Diag,
        "cleanup" => Op::Cleanup,
        "speed" => Op::Speed,
        "dupes" => Op::Dupes,
        "defender" => Op::Defender,
        "repair" => Op::Repair,
        _ => return None,
    })
}

/// Asks Defender to stop its running scan (killing PowerShell alone leaves it running).
pub fn cancel_defender_scan() {
    let exe = std::env::var_os("ProgramFiles")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Program Files"))
        .join(r"Windows Defender\MpCmdRun.exe");
    if exe.is_file() {
        let _ = util::run_hidden(&exe.to_string_lossy(), &["-Scan", "-Cancel"]);
    }
}

#[tauri::command]
pub fn op_cancel(op: String) -> Res<()> {
    let o = parse(&op).ok_or_else(|| tr("Неизвестная операция", "Unknown operation"))?;
    flag(o).store(true, Ordering::SeqCst);
    if o == Op::Defender || (o == Op::Diag && DIAG_IN_DEFENDER.load(Ordering::SeqCst)) {
        cancel_defender_scan();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_separate() {
        begin(Op::Speed);
        begin(Op::Dupes);
        op_cancel("speed".into()).unwrap();
        assert!(cancelled(Op::Speed));
        assert!(!cancelled(Op::Dupes));
        begin(Op::Speed);
        assert!(!cancelled(Op::Speed));
        assert!(op_cancel("nope".into()).is_err());
    }
}
