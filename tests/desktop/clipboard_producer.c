#define _POSIX_C_SOURCE 200809L
/* Disposable data-control source with real text and optional sensitive metadata. */
#include <errno.h>
#include <stdbool.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <wayland-client.h>
#include "wlr-data-control-client-protocol.h"
static struct zwlr_data_control_manager_v1 *manager;
static struct wl_seat *seat;
static const char *payload;
static int delay_ms;
static bool rich;
static void global(void *data, struct wl_registry *registry, uint32_t name,
        const char *interface, uint32_t version) {
    (void)data;
    if (!strcmp(interface, "zwlr_data_control_manager_v1"))
        manager = wl_registry_bind(registry, name, &zwlr_data_control_manager_v1_interface, version < 2 ? version : 2);
    else if (!strcmp(interface, "wl_seat") && !seat)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
}
static void removed(void *data, struct wl_registry *registry, uint32_t name) {(void)data;(void)registry;(void)name;}
static const struct wl_registry_listener registry_listener = {global, removed};
static void send_data(void *data, struct zwlr_data_control_source_v1 *source, const char *mime, int fd) {
    (void)data;(void)source;
    printf("send %s\n",mime);fflush(stdout);
    struct timespec delay={delay_ms/1000,(delay_ms%1000)*1000000L};
    nanosleep(&delay,NULL);
    const char *text = rich && !strcmp(mime,"text/html") ? "<b>Synthetic rich payload</b>" : !strncmp(mime, "text/", 5) ? payload : "1";
    size_t left = strlen(text);
    while (left) {
        ssize_t n = write(fd, text, left);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) break;
        text += n; left -= (size_t)n;
    }
    close(fd);
}
static void cancelled(void *data, struct zwlr_data_control_source_v1 *source) {(void)data;(void)source;puts("cancelled");fflush(stdout);}
static const struct zwlr_data_control_source_v1_listener source_listener = {send_data, cancelled};
int main(int argc, char **argv) {
    if (argc < 2 || argc > 4) return 2;
    payload = argv[1];
    rich=!strcmp(payload,"@html-first")||!strcmp(payload,"@html-only");
    if(rich)payload="Synthetic plain preferred";
    if (!strcmp(payload,"@oversize")) { char *large=malloc(262146);if(!large)return 6;memset(large,'X',262145);large[262145]=0;payload=large; }
    if(argc==4)delay_ms=atoi(argv[3]);
    signal(SIGPIPE, SIG_IGN);
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 3;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    if (wl_display_roundtrip(display) < 0 || !manager || !seat) return 4;
    struct zwlr_data_control_device_v1 *device = zwlr_data_control_manager_v1_get_data_device(manager, seat);
    struct zwlr_data_control_source_v1 *source = zwlr_data_control_manager_v1_create_data_source(manager);
    zwlr_data_control_source_v1_add_listener(source, &source_listener, NULL);
    if(rich)zwlr_data_control_source_v1_offer(source,"text/html");
    if(!strcmp(argv[1],"@html-first"))zwlr_data_control_source_v1_offer(source,"text/plain");
    else if(!rich){zwlr_data_control_source_v1_offer(source,"text/plain;charset=utf-8");zwlr_data_control_source_v1_offer(source,"text/plain");}
    if (argc >= 3 && strcmp(argv[2],"-")) zwlr_data_control_source_v1_offer(source, argv[2]);
    zwlr_data_control_device_v1_set_selection(device, source);
    if (wl_display_flush(display) < 0) return 5;
    puts("ready");fflush(stdout);
    while (wl_display_dispatch(display) >= 0) {}
    wl_display_disconnect(display);
    return 0;
}
