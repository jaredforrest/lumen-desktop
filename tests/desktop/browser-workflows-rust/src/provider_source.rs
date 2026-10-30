//! Narrow pinned upstream API-contract diagnostic, not a provider repair.
#[cfg(test)]
mod tests {
    use std::fs;

    fn body<'a>(source: &'a str, signature: &str) -> &'a str {
        let rest = source
            .split_once(signature)
            .expect("Pinned upstream function")
            .1;
        rest.split_once('{').unwrap().1.split_once('}').unwrap().0
    }

    #[test]
    fn pinned_direct_chooser_response_contract_is_incompatible() {
        let browser = fs::read_to_string(env!("LUMEN_BROWSER_GTK_SOURCE")).unwrap();
        let gtk = fs::read_to_string(env!("LUMEN_GTK_CHOOSER_SOURCE")).unwrap();
        let header = fs::read_to_string(env!("LUMEN_GTK_DIALOG_HEADER")).unwrap();
        let definition = browser
            .split_once("constexpr GtkResponseType kResponseTypeAccept =")
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0;
        let response: i32 = definition
            .trim()
            .strip_prefix("static_cast<GtkResponseType>(")
            .unwrap()
            .strip_suffix(')')
            .unwrap()
            .parse()
            .unwrap();
        let predicate = body(&gtk, "is_stock_accept_response_id (gint response_id)");
        let expression = predicate
            .trim()
            .strip_prefix("return (")
            .unwrap()
            .strip_suffix(";")
            .unwrap()
            .strip_suffix(')')
            .unwrap();
        let accepted = expression
            .split("||")
            .map(|clause| {
                let name = clause
                    .trim()
                    .strip_prefix("response_id == ")
                    .expect("Unsupported upstream predicate grammar");
                let value = header
                    .lines()
                    .find_map(|line| {
                        let (left, right) = line.split_once('=')?;
                        (left.trim() == name)
                            .then(|| right.trim().trim_end_matches(',').parse::<i32>().unwrap())
                    })
                    .expect("Pinned GTK response enum");
                (name, value)
            })
            .collect::<Vec<_>>();
        assert_eq!(response, 0);
        assert!(
            !accepted.iter().any(|(_, value)| *value == response),
            "Provider source changed: reassess fallback acceptance instead of silently retaining the known diagnostic"
        );
        assert!(
            accepted
                .iter()
                .any(|(name, value)| *name == "GTK_RESPONSE_ACCEPT" && *value == -3)
        );
        let save = body(&browser, "SelectFileDialogLinuxGtk::CreateSaveAsDialog(");
        assert!(save.contains("GetSaveLabel(),\n      kResponseTypeAccept"));
        assert!(save.contains("gtk_file_chooser_set_do_overwrite_confirmation"));
        let callback = body(&gtk, "response_cb (GtkDialog *dialog,");
        assert!(callback.contains("is_stock_accept_response_id (response_id)"));
        assert!(callback.contains("_gtk_file_chooser_embed_should_respond"));
        // Replacing all IDs indiscriminately is not an accepted patch: the
        // browser deliberately avoids GTK's stock default-button selection.
        assert!(browser.contains("exploit where the user is instructed to hold Enter"));
    }
}
