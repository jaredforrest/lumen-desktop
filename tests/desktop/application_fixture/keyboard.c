#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include "virtual-keyboard-unstable-v1-client-protocol.h"

struct state {
 struct wl_display *display;
 struct wl_seat *seat;
 struct zwp_virtual_keyboard_manager_v1 *manager;
};
static void global(void *data, struct wl_registry *registry, uint32_t name,
 const char *interface, uint32_t version) {
 struct state *state=data;
 if(!strcmp(interface,wl_seat_interface.name))state->seat=wl_registry_bind(registry,name,&wl_seat_interface,version<7?version:7);
 else if(!strcmp(interface,zwp_virtual_keyboard_manager_v1_interface.name))state->manager=wl_registry_bind(registry,name,&zwp_virtual_keyboard_manager_v1_interface,1);
}
static void removed(void *data,struct wl_registry *registry,uint32_t name) {(void)data;(void)registry;(void)name;}
static const struct wl_registry_listener listener={global,removed};
int main(void) {
 struct state state={0};state.display=wl_display_connect(NULL);
 if(!state.display){fprintf(stderr,"private display unavailable\n");return 1;}
 struct wl_registry *registry=wl_display_get_registry(state.display);
 wl_registry_add_listener(registry,&listener,&state);
 if(wl_display_roundtrip(state.display)<0 || !state.seat || !state.manager){fprintf(stderr,"private virtual keyboard unavailable\n");return 1;}
 struct zwp_virtual_keyboard_v1 *keyboard=zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(state.manager,state.seat);
 struct xkb_context *context=xkb_context_new(XKB_CONTEXT_NO_FLAGS);
 struct xkb_rule_names names={.layout="us"};
 struct xkb_keymap *map=context?xkb_keymap_new_from_names(context,&names,XKB_KEYMAP_COMPILE_NO_FLAGS):NULL;
 char *text=map?xkb_keymap_get_as_string(map,XKB_KEYMAP_FORMAT_TEXT_V1):NULL;
 int fd=memfd_create("private-physical-keyboard",MFD_CLOEXEC);
 if(!text || fd<0 || write(fd,text,strlen(text)+1)!=(ssize_t)(strlen(text)+1)){fprintf(stderr,"stable keymap unavailable\n");return 1;}
 zwp_virtual_keyboard_v1_keymap(keyboard,WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1,fd,strlen(text)+1);
 if(wl_display_roundtrip(state.display)<0)return 1;
 close(fd);free(text);xkb_keymap_unref(map);xkb_context_unref(context);
 puts("ready");fflush(stdout);
 char line[128];int result=0;
 while(fgets(line,sizeof(line),stdin)) {
  char op,extra;unsigned int serial,value,pressed;
  if(!strchr(line,'\n') || sscanf(line," %c %u %u %u %c",&op,&serial,&value,&pressed,&extra)!=4 || !serial ||
   (op!='k' && op!='m') || (op=='k' && (value>255 || pressed>1)) || (op=='m' && ((value&~(1u|4u|8u|64u)) || pressed))) {
   fprintf(stderr,"invalid keyboard command; no acknowledgement\n");result=2;break;
  }
  if(op=='k')zwp_virtual_keyboard_v1_key(keyboard,0,value,pressed);
  else zwp_virtual_keyboard_v1_modifiers(keyboard,value,0,0,0);
  if(wl_display_roundtrip(state.display)<0){result=1;break;}
  printf("ack %u\n",serial);fflush(stdout);
 }
 zwp_virtual_keyboard_v1_modifiers(keyboard,0,0,0,0);
 zwp_virtual_keyboard_v1_destroy(keyboard);zwp_virtual_keyboard_manager_v1_destroy(state.manager);wl_seat_destroy(state.seat);wl_registry_destroy(registry);wl_display_roundtrip(state.display);wl_display_disconnect(state.display);
 return result;
}
