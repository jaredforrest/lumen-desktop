fn main() {
    if std::env::var_os("CARGO_FEATURE_EDS").is_some() {
        for library in [
            "libecal-2.0",
            "libedataserver-1.2",
            "goa-1.0",
            "libical-glib",
        ] {
            pkg_config::probe_library(library).expect("native calendar library is required");
        }
    }
}
