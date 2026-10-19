#include <gtk/gtk.h>
#include <gmodule.h>

static GSettings *appearance;

static gboolean
map_color_scheme (GValue *value, GVariant *variant, gpointer user_data)
{
  (void) user_data;
  g_value_set_boolean (value,
    g_str_equal (g_variant_get_string (variant, NULL), "prefer-dark"));
  return TRUE;
}

/* Loaded only by the authentication-agent wrapper, without changing auth code. */
G_MODULE_EXPORT void
gtk_module_init (gint *argc, gchar ***argv)
{
  GSettingsSchemaSource *source;
  GSettingsSchema *schema;
  GtkSettings *settings;
  (void) argc;
  (void) argv;
  if (appearance != NULL)
    return;
  source = g_settings_schema_source_new_from_directory (
    DESKTOP_SCHEMA_DIR, g_settings_schema_source_get_default (), FALSE, NULL);
  if (source == NULL)
    return;
  schema = g_settings_schema_source_lookup (source,
    "org.gnome.desktop.interface", TRUE);
  g_settings_schema_source_unref (source);
  if (schema == NULL)
    return;
  settings = gtk_settings_get_default ();
  appearance = g_settings_new_full (schema, NULL, NULL);
  g_settings_bind (appearance, "gtk-theme", settings,
    "gtk-theme-name", G_SETTINGS_BIND_GET);
  if (g_settings_schema_has_key (schema, "color-scheme"))
    g_settings_bind_with_mapping (appearance, "color-scheme", settings,
      "gtk-application-prefer-dark-theme", G_SETTINGS_BIND_GET,
      map_color_scheme, NULL, NULL, NULL);
  g_settings_schema_unref (schema);
}
