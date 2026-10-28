//! Disposable GTK save dialog for real compositor decoration inspection.
use gtk::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    gtk::init()?;
    let dialog = gtk::FileChooserDialog::new(
        Some("Save Dialog Fixture"),
        None::<&gtk::Window>,
        gtk::FileChooserAction::Save,
    );
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Save", gtk::ResponseType::Accept);
    dialog.set_default_size(760, 520);
    dialog.set_current_name("fixture.html");
    dialog.show_all();
    gtk::main();
    Ok(())
}
