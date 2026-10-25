use crate::Progress;
use std::time::{Duration, Instant};

pub(crate) struct Reporter<'a> {
    callback: &'a mut dyn FnMut(Progress),
    state: Progress,
    last: Instant,
}
impl<'a> Reporter<'a> {
    pub(crate) fn new(callback: &'a mut dyn FnMut(Progress)) -> Self {
        Self {
            callback,
            state: Progress::default(),
            last: Instant::now(),
        }
    }
    pub(crate) fn stage(
        &mut self,
        stage: &str,
        indeterminate: bool,
        total: u64,
        scanned: u64,
        found: usize,
    ) {
        self.state.stage = stage.into();
        self.state.indeterminate = indeterminate;
        self.state.total = total;
        self.state.scanned = scanned;
        self.state.found = found;
        self.emit(true);
    }
    pub(crate) fn io(&mut self, bytes: u64, offset: Option<u64>) {
        self.state.bytes_read = bytes;
        self.state.current_offset = offset;
        self.emit(false);
    }
    pub(crate) fn position(&mut self, scanned: u64, found: usize) {
        self.state.scanned = scanned;
        self.state.found = found;
        self.emit(false);
    }
    pub(crate) fn emit(&mut self, force: bool) {
        if force || self.last.elapsed() >= Duration::from_millis(100) {
            (self.callback)(self.state.clone());
            self.last = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn io_heartbeats_are_throttled_but_stage_transitions_are_immediate() {
        let events = RefCell::new(Vec::new());
        let mut callback = |p: Progress| events.borrow_mut().push(p);
        let mut reporter = Reporter::new(&mut callback);
        reporter.stage("metadata", true, 0, 0, 0);
        for bytes in 0..1000 {
            reporter.io(bytes, Some(bytes));
        }
        assert_eq!(events.borrow().len(), 1);
        reporter.last = Instant::now() - Duration::from_millis(101);
        reporter.io(1234, Some(5678));
        assert_eq!(events.borrow().len(), 2);
        assert_eq!(events.borrow()[1].bytes_read, 1234);
        reporter.stage("carving", false, 10000, 0, 0);
        assert_eq!(events.borrow().len(), 3);
    }
}
