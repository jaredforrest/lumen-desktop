fn main() -> pyo3::PyResult<()> {
    lumen_gi_deprecation_check::modern_imports_are_quiet_and_legacy_use_reports_once()?;
    lumen_gi_deprecation_check::modern_glib_runner_and_explicit_legacy_policy()?;
    println!("PASS patched GI deprecations and GLib asyncio compatibility");
    Ok(())
}
