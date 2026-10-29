//! Absolute observation budgets for a private AT-SPI provider.
use crate::{Result, fail};
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::time::Duration;

pub const CALL_CAP_SECONDS: f64 = 0.5;
pub const OBSERVATION_SECONDS: f64 = 5.0;
pub const MAX_OWNERS: usize = 256;
pub const MAX_OWNER_BYTES: usize = 1024 * 1024;
pub const MAX_NODES: usize = 2048;
pub const MAX_ANCESTORS: usize = 48;
pub const MAX_TREE_BYTES: usize = 4 * 1024 * 1024;
fn budget_error() -> Box<dyn std::error::Error + Send + Sync> {
    fail("Private accessibility observation exceeded its budget")
}
pub fn bounded_call<T>(
    clock: &mut impl FnMut() -> f64,
    deadline: Option<f64>,
    method: impl FnOnce(Duration) -> Result<T>,
) -> Result<T> {
    let remaining = deadline.map(|end| end - clock());
    if remaining.is_some_and(|time| time <= 0.0) {
        return Err(budget_error());
    }
    let timeout =
        Duration::from_secs_f64(remaining.unwrap_or(CALL_CAP_SECONDS).min(CALL_CAP_SECONDS));
    let value = method(timeout)?;
    if deadline.is_some_and(|end| clock() > end) {
        return Err(budget_error());
    }
    Ok(value)
}
pub trait Provider {
    fn list_names(&mut self, timeout: Duration) -> Result<Vec<String>>;
    fn pid(&mut self, owner: &str, timeout: Duration) -> Result<u32>;
    fn describe(&mut self, reference: &(String, String), timeout: Duration) -> Result<Value>;
    fn children(
        &mut self,
        reference: &(String, String),
        timeout: Duration,
    ) -> Result<Vec<(String, String)>>;
}
pub struct Observer<P, C> {
    pub provider: P,
    pub clock: C,
}
impl<P: Provider, C: FnMut() -> f64> Observer<P, C> {
    pub fn diagnostics(&mut self) -> Result<Vec<Value>> {
        let deadline = (self.clock)() + OBSERVATION_SECONDS;
        let names = bounded_call(&mut self.clock, Some(deadline), |timeout| {
            self.provider.list_names(timeout)
        })?;
        if names.len() > MAX_OWNERS {
            return Err(fail(
                "Private accessibility owner list exceeds count budget",
            ));
        }
        let mut rows = Vec::new();
        for owner in names {
            if !owner.starts_with(':') {
                continue;
            }
            let row = match bounded_call(&mut self.clock, Some(deadline), |timeout| {
                self.provider.pid(&owner, timeout)
            }) {
                Ok(pid) => json!({"owner":owner,"pid":pid}),
                Err(error) => json!({"owner":owner,"unavailable":error.to_string()}),
            };
            rows.push(row);
            if (self.clock)() > deadline {
                return Err(budget_error());
            }
            if serde_json::to_vec(&rows)?.len() > MAX_OWNER_BYTES {
                return Err(fail(
                    "Private accessibility owner records exceed byte budget",
                ));
            }
        }
        Ok(rows)
    }
    pub fn snapshot(&mut self) -> Result<Vec<Value>> {
        let deadline = (self.clock)() + OBSERVATION_SECONDS;
        let mut pending = VecDeque::from([(
            (
                "org.a11y.atspi.Registry".into(),
                "/org/a11y/atspi/accessible/root".into(),
            ),
            Vec::<Value>::new(),
        )]);
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        while let Some((reference, parents)) = pending.pop_front() {
            if rows.len() >= MAX_NODES || (self.clock)() > deadline {
                return Err(budget_error());
            }
            if reference.1 == "/org/a11y/atspi/null" || !seen.insert(reference.clone()) {
                continue;
            }
            match bounded_call(&mut self.clock, Some(deadline), |timeout| {
                self.provider.describe(&reference, timeout)
            }) {
                Ok(mut row) => {
                    row["ancestors"] = json!(parents);
                    rows.push(row.clone());
                    if parents.len() >= MAX_ANCESTORS {
                        return Err(fail("Private accessibility ancestry exceeds budget"));
                    }
                    let children = bounded_call(&mut self.clock, Some(deadline), |timeout| {
                        self.provider.children(&reference, timeout)
                    })?;
                    let mut next = parents;
                    next.push(
                        json!({"reference":row["reference"],"name":row["name"],"role":row["role"]}),
                    );
                    pending.extend(children.into_iter().map(|child| (child, next.clone())));
                }
                Err(error) => rows.push(
                    json!({"reference":[reference.0,reference.1],"unavailable":error.to_string()}),
                ),
            }
            if (self.clock)() > deadline {
                return Err(budget_error());
            }
            if serde_json::to_vec(&rows)?.len() > MAX_TREE_BYTES {
                return Err(fail("Private accessibility snapshot exceeds byte budget"));
            }
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    fn clock(values: Vec<f64>) -> impl FnMut() -> f64 {
        let mut values = VecDeque::from(values);
        move || values.pop_front().unwrap_or(0.0)
    }
    #[test]
    fn remaining_budget_bounds_last_roundtrip() {
        let seen = Rc::new(Cell::new(0.0));
        let copy = seen.clone();
        let value = bounded_call(&mut clock(vec![4.8, 4.9]), Some(5.0), |timeout| {
            copy.set(timeout.as_secs_f64());
            Ok("row")
        })
        .unwrap();
        assert_eq!(value, "row");
        assert!((seen.get() - 0.2).abs() < 1e-9);
    }
    #[test]
    fn late_response_never_succeeds() {
        let value = bounded_call(&mut clock(vec![4.8, 5.01]), Some(5.0), |_| Ok("late"));
        assert!(value.unwrap_err().to_string().contains("budget"));
    }
    #[test]
    fn expired_budget_skips_provider() {
        let called = Cell::new(false);
        let value = bounded_call(&mut clock(vec![5.0]), Some(5.0), |_| {
            called.set(true);
            Ok(())
        });
        assert!(value.is_err());
        assert!(!called.get());
    }
    #[test]
    fn individual_cap_is_half_second() {
        let seen = Cell::new(0.0);
        bounded_call(&mut clock(vec![1.0, 1.2]), Some(5.0), |timeout| {
            seen.set(timeout.as_secs_f64());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen.get(), 0.5);
    }
    struct Fake {
        names: Vec<String>,
        pid_error: bool,
        describe_error: bool,
        pid_calls: usize,
        list_timeout: f64,
    }
    impl Provider for Fake {
        fn list_names(&mut self, timeout: Duration) -> Result<Vec<String>> {
            self.list_timeout = timeout.as_secs_f64();
            Ok(self.names.clone())
        }
        fn pid(&mut self, _: &str, _: Duration) -> Result<u32> {
            self.pid_calls += 1;
            if self.pid_error {
                Err(fail("late"))
            } else {
                Ok(123)
            }
        }
        fn describe(&mut self, _: &(String, String), _: Duration) -> Result<Value> {
            if self.describe_error {
                Err(fail("unavailable"))
            } else {
                Ok(json!({"reference":["owner","/path"],"name":"root","role":"frame"}))
            }
        }
        fn children(&mut self, _: &(String, String), _: Duration) -> Result<Vec<(String, String)>> {
            Ok(Vec::new())
        }
    }
    fn fake(names: Vec<String>) -> Fake {
        Fake {
            names,
            pid_error: false,
            describe_error: false,
            pid_calls: 0,
            list_timeout: 0.0,
        }
    }
    #[test]
    fn owner_list_roundtrip_is_bounded() {
        let mut observer = Observer {
            provider: fake(Vec::new()),
            clock: clock(vec![0.0, 0.0, 0.1]),
        };
        assert!(observer.diagnostics().unwrap().is_empty());
        assert_eq!(observer.provider.list_timeout, 0.5);
    }
    #[test]
    fn oversized_owner_list_rejected_before_pid() {
        let mut observer = Observer {
            provider: fake((0..257).map(|i| format!(":1.{i}")).collect()),
            clock: clock(vec![0.0, 0.0, 0.1]),
        };
        assert!(
            observer
                .diagnostics()
                .unwrap_err()
                .to_string()
                .contains("count budget")
        );
        assert_eq!(observer.provider.pid_calls, 0);
    }
    #[test]
    fn late_owner_error_does_not_escape_budget() {
        let mut provider = fake(vec![":1.2".into()]);
        provider.pid_error = true;
        let mut observer = Observer {
            provider,
            clock: clock(vec![0.0, 0.0, 0.1, 0.1, 5.01]),
        };
        assert!(
            observer
                .diagnostics()
                .unwrap_err()
                .to_string()
                .contains("budget")
        );
        assert_eq!(observer.provider.pid_calls, 1);
    }
    #[test]
    fn unavailable_last_row_cannot_return_after_deadline() {
        let mut provider = fake(Vec::new());
        provider.describe_error = true;
        let mut observer = Observer {
            provider,
            clock: clock(vec![0.0, 0.0, 0.0, 5.01]),
        };
        assert!(
            observer
                .snapshot()
                .unwrap_err()
                .to_string()
                .contains("budget")
        );
    }
}
