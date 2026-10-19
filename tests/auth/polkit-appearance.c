/* Synthetic dialog: no authentication authority or password handling. */
#include <gtk/gtk.h>

int
main (int argc, char **argv)
{
  const gchar *themes[] = { "Adwaita", "Adwaita-dark", "Adwaita", "Adwaita" };
  const gchar *schemes[] = { "default", "prefer-dark", "prefer-light", "default" };
  GtkWidget *dialog;
  GSettings *preferences;
  gint i;
  gtk_init (&argc, &argv);
  preferences = g_settings_new ("org.gnome.desktop.interface");
  dialog = gtk_dialog_new ();
  gtk_window_set_title (GTK_WINDOW (dialog), "Authentication required");
  gtk_container_add (GTK_CONTAINER (gtk_dialog_get_content_area (GTK_DIALOG (dialog))),
                      gtk_label_new ("Protected setting"));
  gtk_widget_show_all (dialog);
  for (i = 0; i < 4; i++)
    {
      gchar *actual_theme;
      gboolean actual_dark;
      g_assert_true (g_settings_set_string (preferences, "gtk-theme", themes[i]));
      g_assert_true (g_settings_set_string (preferences, "color-scheme", schemes[i]));
      while (gtk_events_pending ()) gtk_main_iteration ();
      g_object_get (gtk_settings_get_default (), "gtk-theme-name", &actual_theme,
                    "gtk-application-prefer-dark-theme", &actual_dark, NULL);
      g_assert_cmpstr (actual_theme, ==, themes[i]);
      g_assert_cmpint (actual_dark, ==, i == 1);
      g_free (actual_theme);
      g_assert_true (gtk_widget_get_visible (dialog));
    }
  gtk_widget_destroy (dialog);
  g_object_unref (preferences);
  g_print ("PASS: module initialisation and live light/dark/default theme propagation\n");
  return 0;
}
