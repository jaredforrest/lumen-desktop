pub fn value() -> i32 {
    #[cfg(feature = "warning")]
    let policy_dependency_unused = 42;
    42
}
