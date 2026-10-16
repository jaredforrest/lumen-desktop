/* GTK3 custom-surface bridge for input-method-v2. Normal-field text is bounded in memory and cleared on field changes; never logged. */
#include <gtk/gtk.h>
#include <gdk/gdkwayland.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include <sys/mman.h>
#include <unistd.h>
#include <string.h>
#include "input-method-client.h"

static struct wl_display *display;
static struct wl_seat *seat;
static struct zwp_input_method_manager_v2 *manager;
static struct zwp_input_method_v2 *method;
static struct zwp_input_popup_surface_v2 *popup;
static struct zwp_input_method_keyboard_grab_v2 *grab;
static struct xkb_context *context;
static struct xkb_keymap *keymap;
static struct xkb_state *keys;
static GtkWidget *window;
static unsigned int serial, epoch, pending_epoch;
static gboolean available, active, pending_active, positioned;
static void (*changed)(void);
static void (*replacement)(const char *, uint32_t, unsigned int, unsigned int);
static char *pending_text, *previous_text;
static uint32_t cursor, anchor, previous_cursor, hint, purpose, change_cause;
static gboolean text_updated;
static void clear_text(void) {
    g_clear_pointer(&pending_text,g_free);g_clear_pointer(&previous_text,g_free);
    cursor=anchor=previous_cursor=0;text_updated=FALSE;
}
void cv_replacement_callback(void (*callback)(const char *, uint32_t, unsigned int, unsigned int)) {replacement=callback;}

static guint repeat_timer;
static uint32_t repeating_key;
static int repeat_rate = 25, repeat_delay = 500;

static void stop_repeat(void) {
    if (repeat_timer) g_source_remove(repeat_timer);
    repeat_timer = 0;
}
void cv_hide(void) {
    stop_repeat();
    window = NULL;
    if (grab) zwp_input_method_keyboard_grab_v2_release(grab);
    grab = NULL;
    if (popup) zwp_input_popup_surface_v2_destroy(popup);
    popup = NULL;
    if (display) wl_display_flush(display);
}
static void activate(void *d, struct zwp_input_method_v2 *im) {
    clear_text();hint=purpose=change_cause=0;
    pending_active = TRUE;
    pending_epoch++;
}
static void deactivate(void *d, struct zwp_input_method_v2 *im) { pending_active = FALSE;clear_text(); }
static void surrounding(void *d, struct zwp_input_method_v2 *im, const char *s, uint32_t c, uint32_t a) {
    g_clear_pointer(&pending_text,g_free);text_updated=TRUE;
    if (strlen(s)<=4096 && g_utf8_validate(s,-1,NULL) && c<=strlen(s) && a<=strlen(s)) {
        pending_text=g_strdup(s);cursor=c;anchor=a;
    }
}
static void cause(void *d, struct zwp_input_method_v2 *im, uint32_t c) {change_cause=c;}
static void content(void *d, struct zwp_input_method_v2 *im, uint32_t h, uint32_t p) {hint=h;purpose=p;if((h & (0x40|0x80)) || !(p==0 || p==1 || p==7))clear_text();}
static void done(void *d, struct zwp_input_method_v2 *im) {
    serial++;
    gboolean reset = active != pending_active || epoch != pending_epoch;
    active = pending_active;
    epoch = pending_epoch;
    if (reset && changed) changed();
    if (!active || (hint & (0x40|0x80)) || !(purpose==0 || purpose==1 || purpose==7)) {clear_text();return;}
    if (text_updated && pending_text) {
        // Only a newly inserted single space/tab/newline, never a pasted paragraph,
        // cursor movement, activation snapshot, selection or our own replacement.
        if (!reset && !window && replacement && change_cause==0 && previous_text && cursor==anchor &&
            cursor==previous_cursor+1 && strlen(pending_text)==strlen(previous_text)+1 &&
            !strncmp(pending_text,previous_text,previous_cursor) &&
            !strcmp(pending_text+cursor,previous_text+previous_cursor) &&
            strchr(" \t\n",pending_text[cursor-1])) replacement(pending_text,cursor,epoch,serial);
        g_free(previous_text);previous_text=g_strdup(pending_text);previous_cursor=cursor;
    }
    text_updated=FALSE;change_cause=0;
}
static void unavailable(void *d, struct zwp_input_method_v2 *im) {
    active = pending_active = available = FALSE;clear_text();
    if (changed) changed();
}
static const struct zwp_input_method_v2_listener listener = {
    .activate = activate, .deactivate = deactivate, .surrounding_text = surrounding,
    .text_change_cause = cause, .content_type = content, .done = done, .unavailable = unavailable
};
static void global(void *d, struct wl_registry *r, uint32_t id, const char *name, uint32_t v) {
    if (!strcmp(name, "wl_seat") && !seat)
        seat = wl_registry_bind(r, id, &wl_seat_interface, 1);
    if (!strcmp(name, "zwp_input_method_manager_v2"))
        manager = wl_registry_bind(r, id, &zwp_input_method_manager_v2_interface, 1);
}
static void removed(void *d, struct wl_registry *r, uint32_t id) {}
static const struct wl_registry_listener registry_listener = {global, removed};
int cv_init(void (*callback)(void)) {
    GdkDisplay *gd = gdk_display_get_default();
    if (!GDK_IS_WAYLAND_DISPLAY(gd)) return 0;
    display = gdk_wayland_display_get_wl_display(gd);
    changed = callback;
    context = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!manager || !seat) return 0;
    method = zwp_input_method_manager_v2_get_input_method(manager, seat);
    available = TRUE;
    zwp_input_method_v2_add_listener(method, &listener, NULL);
    wl_display_roundtrip(display);
    return available;
}
int cv_active(void) { return available && active; }
int cv_available(void) { return available; }
int cv_positioned(void) { return positioned; }
unsigned int cv_epoch(void) { return epoch; }
static void keymap_event(void *d, struct zwp_input_method_keyboard_grab_v2 *g,
                         uint32_t format, int32_t fd, uint32_t size) {
    if (format != WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1) { close(fd); return; }
    char *map = mmap(NULL, size, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd);
    if (map == MAP_FAILED) return;
    struct xkb_keymap *next = xkb_keymap_new_from_string(context, map, XKB_KEYMAP_FORMAT_TEXT_V1, 0);
    munmap(map, size);
    if (!next) return;
    if (keys) xkb_state_unref(keys);
    if (keymap) xkb_keymap_unref(keymap);
    keymap = next;
    keys = xkb_state_new(keymap);
}
static GdkModifierType mods(void) {
    GdkModifierType m = 0;
    if (xkb_state_mod_name_is_active(keys, XKB_MOD_NAME_SHIFT, XKB_STATE_MODS_EFFECTIVE)) m |= GDK_SHIFT_MASK;
    if (xkb_state_mod_name_is_active(keys, XKB_MOD_NAME_CTRL, XKB_STATE_MODS_EFFECTIVE)) m |= GDK_CONTROL_MASK;
    if (xkb_state_mod_name_is_active(keys, XKB_MOD_NAME_ALT, XKB_STATE_MODS_EFFECTIVE)) m |= GDK_MOD1_MASK;
    if (xkb_state_mod_name_is_active(keys, XKB_MOD_NAME_LOGO, XKB_STATE_MODS_EFFECTIVE)) m |= GDK_SUPER_MASK;
    return m;
}
static void dispatch_key(uint32_t key, gboolean pressed, uint32_t time) {
    if (!window || !keys) return;
    GdkEvent *event = gdk_event_new(pressed ? GDK_KEY_PRESS : GDK_KEY_RELEASE);
    event->key.window = g_object_ref(gtk_widget_get_window(window));
    event->key.send_event = TRUE;
    event->key.time = time;
    event->key.state = mods();
    event->key.keyval = xkb_state_key_get_one_sym(keys, key + 8);
    event->key.hardware_keycode = key + 8;
    event->key.group = xkb_state_key_get_layout(keys, key + 8);
    char text[128];
    int n = xkb_state_key_get_utf8(keys, key + 8, text, sizeof(text));
    event->key.string = g_strndup(text, n > 0 ? MIN(n, 127) : 0);
    event->key.length = strlen(event->key.string);
    GdkDevice *device = gdk_seat_get_keyboard(gdk_display_get_default_seat(gdk_display_get_default()));
    gdk_event_set_device(event, device);
    gtk_widget_event(window, event);
    gdk_event_free(event);
}
static gboolean repeat(void *d) {
    if (!window) { repeat_timer = 0; return G_SOURCE_REMOVE; }
    dispatch_key(repeating_key, TRUE, GDK_CURRENT_TIME);
    return G_SOURCE_CONTINUE;
}
static gboolean begin_repeat(void *d) {
    repeat_timer = 0;
    if (!window) return G_SOURCE_REMOVE;
    dispatch_key(repeating_key, TRUE, GDK_CURRENT_TIME);
    if (window) repeat_timer = g_timeout_add(MAX(1, 1000 / repeat_rate), repeat, NULL);
    return G_SOURCE_REMOVE;
}
static void key_event(void *d, struct zwp_input_method_keyboard_grab_v2 *g,
                     uint32_t s, uint32_t time, uint32_t key, uint32_t state) {
    gboolean pressed = state == WL_KEYBOARD_KEY_STATE_PRESSED;
    if (pressed || key == repeating_key) stop_repeat();
    dispatch_key(key, pressed, time);
    if (pressed && window && keymap && repeat_rate > 0 && xkb_keymap_key_repeats(keymap, key + 8)) {
        repeating_key = key;
        repeat_timer = g_timeout_add(MAX(1, repeat_delay), begin_repeat, NULL);
    }
}
static void modifiers(void *d, struct zwp_input_method_keyboard_grab_v2 *g,
                      uint32_t s, uint32_t depressed, uint32_t latched, uint32_t locked, uint32_t group) {
    if (keys) xkb_state_update_mask(keys, depressed, latched, locked, 0, 0, group);
}
static void repeat_info(void *d, struct zwp_input_method_keyboard_grab_v2 *g, int32_t r, int32_t delay) {
    repeat_rate = r; repeat_delay = delay;
}
static const struct zwp_input_method_keyboard_grab_v2_listener grab_listener = {
    .keymap = keymap_event, .key = key_event, .modifiers = modifiers, .repeat_info = repeat_info
};
static void rectangle(void *d, struct zwp_input_popup_surface_v2 *p, int32_t x, int32_t y, int32_t w, int32_t h) { positioned = TRUE; }
static const struct zwp_input_popup_surface_v2_listener popup_listener = {rectangle};
int cv_attach(void *widget) {
    if (!cv_active()) return 0;
    positioned = FALSE;
    window = GTK_WIDGET(widget);
    GdkWindow *gd = gtk_widget_get_window(window);
    gdk_wayland_window_set_use_custom_surface(gd);
    popup = zwp_input_method_v2_get_input_popup_surface(method, gdk_wayland_window_get_wl_surface(gd));
    zwp_input_popup_surface_v2_add_listener(popup, &popup_listener, NULL);
    grab = zwp_input_method_v2_grab_keyboard(method);
    zwp_input_method_keyboard_grab_v2_add_listener(grab, &grab_listener, NULL);
    wl_display_flush(display);
    return 1;
}
int cv_commit(const char *text, unsigned int expected_epoch) {
    if (!cv_active() || epoch != expected_epoch) return 0;
    zwp_input_method_v2_commit_string(method, text);
    zwp_input_method_v2_commit(method, serial);
    wl_display_flush(display);
    return 1;
}

int cv_replace(uint32_t before, const char *text, unsigned int expected_epoch, unsigned int expected_serial) {
    if (!cv_active() || window || epoch!=expected_epoch || serial!=expected_serial || before>cursor ||
        (hint & (0x40|0x80)) || !(purpose==0 || purpose==1 || purpose==7)) return 0;
    zwp_input_method_v2_delete_surrounding_text(method,before,0);
    zwp_input_method_v2_commit_string(method,text);
    zwp_input_method_v2_commit(method,serial);
    wl_display_flush(display);return 1;
}
