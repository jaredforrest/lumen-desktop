use pyo3::prelude::*;
use pyo3::types::PyDict;

pub fn modern_imports_are_quiet_and_legacy_use_reports_once() -> PyResult<()> {
    Python::attach(|py| {
        let gi = py.import("gi")?;
        let warnings = py.import("warnings")?;
        let builtins = py.import("builtins")?;
        gi.call_method1("require_version", ("GLib", "2.0"))?;
        gi.call_method1("require_version", ("GioUnix", "2.0"))?;

        let none = py.None();
        let catcher = warnings.call_method0("catch_warnings")?;
        catcher.call_method0("__enter__")?;
        let imported = (|| -> PyResult<_> {
            warnings.call_method1("simplefilter", ("error",))?;
            let repository = builtins.call_method1(
                "__import__",
                ("gi.repository", &none, &none, vec!["GLib", "GioUnix"]),
            )?;
            let glib = repository.getattr("GLib")?;
            let gio_unix = repository.getattr("GioUnix")?;
            assert!(
                builtins
                    .call_method1("callable", (glib.getattr("MainLoop")?,))?
                    .extract::<bool>()?
            );
            assert!(
                builtins
                    .call_method1("callable", (gio_unix.getattr("DesktopAppInfo")?,))?
                    .extract::<bool>()?
            );
            Ok(glib)
        })();
        catcher.call_method1("__exit__", (&none, &none, &none))?;
        let glib = imported?;

        let overrides = py.import("gi.overrides")?;
        let deprecated = overrides.getattr("deprecated_attr")?;
        let registered = overrides.getattr("_deprecated_attrs")?;
        let checked = (|| -> PyResult<()> {
            deprecated.call1(("LumenFixture", "alias", "OldTarget"))?;
            deprecated.call1(("LumenFixture", "other", "OtherTarget"))?;
            deprecated.call1(("LumenFixture", "alias", "LatestTarget"))?;
            let entries: Vec<(String, String)> = registered.get_item("LumenFixture")?.extract()?;
            assert_eq!(
                entries,
                [
                    ("alias".into(), "LatestTarget".into()),
                    ("other".into(), "OtherTarget".into())
                ]
            );
            Ok(())
        })();
        registered.call_method1("pop", ("LumenFixture", &none))?;
        checked?;

        let kwargs = PyDict::new(py);
        kwargs.set_item("record", true)?;
        let catcher = warnings
            .getattr("catch_warnings")?
            .call((), Some(&kwargs))?;
        let diagnostics = catcher.call_method0("__enter__")?;
        let legacy = (|| -> PyResult<_> {
            warnings.call_method1("simplefilter", ("always",))?;
            glib.getattr("unix_signal_add_full")
        })();
        catcher.call_method1("__exit__", (&none, &none, &none))?;
        let legacy = legacy?;
        assert_eq!(diagnostics.len()?, 1);
        let item = diagnostics.get_item(0)?;
        assert!(
            builtins
                .call_method1(
                    "issubclass",
                    (
                        item.getattr("category")?,
                        gi.getattr("PyGIDeprecationWarning")?
                    )
                )?
                .extract::<bool>()?
        );
        assert!(
            item.getattr("message")?
                .str()?
                .to_string()
                .contains("GLibUnix.signal_add")
        );
        assert!(
            builtins
                .call_method1("callable", (legacy,))?
                .extract::<bool>()?
        );
        Ok(())
    })
}

pub fn modern_glib_runner_and_explicit_legacy_policy() -> PyResult<()> {
    Python::attach(|py| {
        let warnings = py.import("warnings")?;
        let builtins = py.import("builtins")?;
        let none = py.None();
        let catcher = warnings.call_method0("catch_warnings")?;
        catcher.call_method0("__enter__")?;
        let modern = (|| -> PyResult<_> {
            warnings.call_method1("simplefilter", ("error",))?;
            let asyncio = py.import("asyncio")?;
            let events = py.import("gi.events")?;
            let vars = builtins.call_method1("vars", (&events,))?;
            assert!(!vars.contains("GLibEventLoopPolicy")?);

            let kwargs = PyDict::new(py);
            kwargs.set_item("loop_factory", events.getattr("GLibEventLoop")?)?;
            let runner = asyncio.getattr("Runner")?.call((), Some(&kwargs))?;
            runner.call_method0("__enter__")?;
            let result = (|| -> PyResult<i32> {
                let kwargs = PyDict::new(py);
                kwargs.set_item("result", 23)?;
                let pending = asyncio.call_method("sleep", (0,), Some(&kwargs))?;
                runner.call_method1("run", (pending,))?.extract()
            })();
            runner.call_method1("__exit__", (&none, &none, &none))?;
            assert_eq!(result?, 23);

            let glib = py.import("gi.repository.GLib")?;
            let main = glib.getattr("MainLoop")?.call0()?;
            glib.call_method1("idle_add", (main.getattr("quit")?,))?;
            main.call_method0("run")?;
            assert!(!vars.contains("GLibEventLoopPolicy")?);
            Ok((asyncio, events))
        })();
        catcher.call_method1("__exit__", (&none, &none, &none))?;
        let (asyncio, events) = modern?;

        let names = builtins.call_method1("dir", (&events,))?;
        assert!(names.contains("GLibEventLoopPolicy")?);
        let kwargs = PyDict::new(py);
        kwargs.set_item("record", true)?;
        let catcher = warnings
            .getattr("catch_warnings")?
            .call((), Some(&kwargs))?;
        let diagnostics = catcher.call_method0("__enter__")?;
        let legacy = (|| -> PyResult<()> {
            warnings.call_method1(
                "simplefilter",
                ("always", builtins.getattr("DeprecationWarning")?),
            )?;
            let policy = events.getattr("GLibEventLoopPolicy")?;
            let name: String = policy.getattr("__qualname__")?.extract()?;
            assert_eq!(name, "GLibEventLoopPolicy");
            let policy = policy.call0()?;
            policy.call_method0("__enter__")?;
            let result = (|| -> PyResult<i32> {
                let kwargs = PyDict::new(py);
                kwargs.set_item("result", 41)?;
                let pending = asyncio.call_method("sleep", (0,), Some(&kwargs))?;
                asyncio
                    .call_method0("get_event_loop")?
                    .call_method1("run_until_complete", (pending,))?
                    .extract()
            })();
            policy.call_method1("__exit__", (&none, &none, &none))?;
            assert_eq!(result?, 41);
            Ok(())
        })();
        catcher.call_method1("__exit__", (&none, &none, &none))?;
        legacy?;
        assert!(diagnostics.len()? > 0);
        let mut policy_warning = false;
        for item in diagnostics.try_iter()? {
            let item = item?;
            let category = item.getattr("category")?;
            assert!(
                builtins
                    .call_method1(
                        "issubclass",
                        (category, builtins.getattr("DeprecationWarning")?)
                    )?
                    .extract::<bool>()?
            );
            let message = item.getattr("message")?.str()?.to_string();
            if message.contains("AbstractEventLoopPolicy") {
                policy_warning = true;
            }
        }
        assert!(policy_warning);
        Ok(())
    })
}
