/* Multi-format file clipboard using Sway's data-control protocol. */
#define _GNU_SOURCE
#include <wayland-client.h>
#include "data-control.h"
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static struct wl_seat *seat;
static struct zwlr_data_control_manager_v1 *manager;
static char *uris, *gnome;
static int cutting, finished;

static void registry_global(void *data, struct wl_registry *registry, uint32_t name,
                            const char *interface, uint32_t version) {
    if (!strcmp(interface, "wl_seat") && !seat)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    else if (!strcmp(interface, "zwlr_data_control_manager_v1"))
        manager = wl_registry_bind(registry, name, &zwlr_data_control_manager_v1_interface, 1);
}
static void registry_remove(void *data, struct wl_registry *registry, uint32_t name) {}
static const struct wl_registry_listener registry_listener = {registry_global, registry_remove};

static void send_data(void *data, struct zwlr_data_control_source_v1 *source, const char *mime, int fd) {
    const char *payload = !strcmp(mime, "x-special/gnome-copied-files") ? gnome :
        !strcmp(mime, "application/x-kde-cutselection") ? (cutting ? "1" : "0") : uris;
    size_t left = strlen(payload);
    while (left) {
        ssize_t written = write(fd, payload, left);
        if (written < 0 && errno == EINTR) continue;
        if (written <= 0) break;
        payload += written; left -= written;
    }
    close(fd);
}
static void cancelled(void *data, struct zwlr_data_control_source_v1 *source) {finished = 1;}
static const struct zwlr_data_control_source_v1_listener source_listener = {send_data, cancelled};
static void offered_mime(void *data, struct zwlr_data_control_offer_v1 *offer, const char *mime) {}
static const struct zwlr_data_control_offer_v1_listener offer_listener = {offered_mime};
static void offer(void *data, struct zwlr_data_control_device_v1 *device, struct zwlr_data_control_offer_v1 *offer) {
    zwlr_data_control_offer_v1_add_listener(offer, &offer_listener, NULL);
}
static void selection(void *data, struct zwlr_data_control_device_v1 *device, struct zwlr_data_control_offer_v1 *offer) {
    if (offer) zwlr_data_control_offer_v1_destroy(offer);
}
static void device_finished(void *data, struct zwlr_data_control_device_v1 *device) {finished = 1;}
static const struct zwlr_data_control_device_v1_listener device_listener = {offer, selection, device_finished};

int main(int argc, char **argv) {
    if (argc < 3 || (strcmp(argv[1], "copy") && strcmp(argv[1], "move"))) return 2;
    cutting = !strcmp(argv[1], "move");
    size_t size = 1;
    for (int i=2; i<argc; ++i) {
        if (strncmp(argv[i], "file:///", 8) || strchr(argv[i], '\n') || strchr(argv[i], '\r')) return 2;
        size += strlen(argv[i]) + 2;
    }
    uris = calloc(size, 1);
    gnome = calloc(size + 8, 1);
    if (!uris || !gnome) return 1;
    strcpy(gnome, cutting ? "cut\n" : "copy\n");
    for (int i=2; i<argc; ++i) {
        strcat(uris, argv[i]); strcat(uris, "\r\n");
        strcat(gnome, argv[i]); if (i+1<argc) strcat(gnome, "\n");
    }
    signal(SIGPIPE, SIG_IGN);
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) return 1;
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    if (wl_display_roundtrip(display) < 0 || !seat || !manager) return 1;
    struct zwlr_data_control_device_v1 *device = zwlr_data_control_manager_v1_get_data_device(manager, seat);
    zwlr_data_control_device_v1_add_listener(device, &device_listener, NULL);
    struct zwlr_data_control_source_v1 *source = zwlr_data_control_manager_v1_create_data_source(manager);
    zwlr_data_control_source_v1_add_listener(source, &source_listener, NULL);
    zwlr_data_control_source_v1_offer(source, "text/uri-list");
    zwlr_data_control_source_v1_offer(source, "x-special/gnome-copied-files");
    zwlr_data_control_source_v1_offer(source, "application/x-kde-cutselection");
    zwlr_data_control_device_v1_set_selection(device, source);
    if (wl_display_roundtrip(display) < 0) return 1;
    puts("ready"); fflush(stdout);
    while (!finished && wl_display_dispatch(display) >= 0) {}
    wl_display_disconnect(display);
    free(uris); free(gnome);
    return 0;
}
