#include <stdio.h>
#include <wayland-server-core.h>
/* Real disposable Wayland endpoint, deliberately without data-control support. */
int main(void) {
 struct wl_display *display=wl_display_create();if(!display)return 1;
 const char *socket=wl_display_add_socket_auto(display);if(!socket)return 2;
 puts(socket);fflush(stdout);wl_display_run(display);wl_display_destroy(display);return 0;
}
