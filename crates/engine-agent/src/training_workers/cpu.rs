//! Linux aggregate CPU counters. Guest time is already included in user/nice;
//! idle and I/O wait are excluded from busy time, like DirtSim's sampler.

#[derive(Clone, Copy)]
struct Counters {
    busy: u64,
    total: u64,
}

fn parse(text: &str) -> Option<Counters> {
    let mut fields = text.lines().next()?.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values = fields
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 8 {
        return None;
    }
    let total = values
        .iter()
        .try_fold(0_u64, |sum, v| sum.checked_add(*v))?;
    let busy = total.checked_sub(values[3])?.checked_sub(values[4])?;
    Some(Counters { busy, total })
}

fn read() -> Option<Counters> {
    #[cfg(target_os = "linux")]
    {
        parse(&std::fs::read_to_string("/proc/stat").ok()?)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

pub(super) struct Sampler {
    previous: Option<Counters>,
}
impl Sampler {
    pub(super) fn new() -> Self {
        Self { previous: read() }
    }
    pub(super) fn sample(&mut self) -> Option<f64> {
        self.update(read())
    }
    fn update(&mut self, current: Option<Counters>) -> Option<f64> {
        let previous = std::mem::replace(&mut self.previous, current)?;
        let current = current?;
        let total = current.total.checked_sub(previous.total)?;
        let busy = current.busy.checked_sub(previous.busy)?;
        (total > 0 && busy <= total).then(|| 100.0 * busy as f64 / total as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_ignore_guest_and_iowait_and_recover_after_missing_samples() {
        let a = parse("cpu 10 0 10 60 20 0 0 0 999 999\ncpu0 0").unwrap();
        let b = parse("cpu 30 0 20 120 30 0 0 0 999 999").unwrap();
        let mut sampler = Sampler { previous: None };
        assert_eq!(sampler.update(Some(a)), None);
        assert_eq!(sampler.update(Some(b)), Some(30.0));
        assert_eq!(sampler.update(Some(b)), None);
        assert_eq!(sampler.update(Some(a)), None); // counter rollback
        assert_eq!(sampler.update(None), None);
        assert_eq!(sampler.update(Some(a)), None);
        assert_eq!(sampler.update(Some(b)), Some(30.0));
        assert!(parse("cpu nope").is_none());
        assert!(parse("cpu0 1 1 1 1 1 1 1 1").is_none());
        assert!(parse("cpu 18446744073709551615 1 1 1 1 1 1 1").is_none());
    }
}
