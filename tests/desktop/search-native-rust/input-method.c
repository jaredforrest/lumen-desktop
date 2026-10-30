// Disposable Wayland input-method protocol driver; no desktop or account access.
#include <wayland-client.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include "input-method-client.h"
static struct wl_display *display;
static struct wl_seat *seat;
static struct zwp_input_method_manager_v2 *manager;
static struct zwp_input_method_v2 *method;
static struct zwp_input_method_keyboard_grab_v2 *grab;
static uint32_t serial;
static int active,pending,unavailable,composing;
static void activate(void *d,struct zwp_input_method_v2 *m){(void)d;(void)m;pending=1;}
static void deactivate(void *d,struct zwp_input_method_v2 *m){(void)d;(void)m;pending=0;}
static void surrounding(void *d,struct zwp_input_method_v2 *m,const char *s,uint32_t c,uint32_t a){(void)d;(void)m;(void)s;(void)c;(void)a;}
static void cause(void *d,struct zwp_input_method_v2 *m,uint32_t c){(void)d;(void)m;(void)c;}
static void content(void *d,struct zwp_input_method_v2 *m,uint32_t h,uint32_t p){(void)d;(void)m;(void)h;(void)p;}
static void done(void *d,struct zwp_input_method_v2 *m){(void)d;(void)m;serial++;active=pending;printf("state %u %d\n",serial,active);fflush(stdout);}
static void lost(void *d,struct zwp_input_method_v2 *m){(void)d;(void)m;unavailable=1;}
static const struct zwp_input_method_v2_listener listener={activate,deactivate,surrounding,cause,content,done,lost};
static void global(void *d,struct wl_registry *r,uint32_t id,const char *name,uint32_t v){(void)d;(void)v;if(!strcmp(name,"wl_seat") && !seat)seat=wl_registry_bind(r,id,&wl_seat_interface,1);if(!strcmp(name,"zwp_input_method_manager_v2"))manager=wl_registry_bind(r,id,&zwp_input_method_manager_v2_interface,1);}
static void removed(void *d,struct wl_registry *r,uint32_t id){(void)d;(void)r;(void)id;}
static const struct wl_registry_listener registry_listener={global,removed};
static void keymap(void *d,struct zwp_input_method_keyboard_grab_v2 *g,uint32_t f,int32_t fd,uint32_t size){(void)d;(void)g;(void)f;(void)size;close(fd);}
static void key(void *d,struct zwp_input_method_keyboard_grab_v2 *g,uint32_t s,uint32_t time,uint32_t key,uint32_t state){
 (void)d;(void)g;(void)s;(void)time;
 if(!state || !composing)return;
 if(key==28){zwp_input_method_v2_commit_string(method,"日本語");zwp_input_method_v2_set_preedit_string(method,"",0,0);zwp_input_method_v2_commit(method,serial);composing=0;puts("candidate commit");}
 else if(key==1){zwp_input_method_v2_set_preedit_string(method,"",0,0);zwp_input_method_v2_commit(method,serial);composing=0;puts("candidate cancel");}
 else if(key==103 || key==108 || key==105 || key==106)printf("candidate arrow %u\n",key);
 fflush(stdout);
}
static void modifiers(void *d,struct zwp_input_method_keyboard_grab_v2 *g,uint32_t s,uint32_t a,uint32_t b,uint32_t c,uint32_t e){(void)d;(void)g;(void)s;(void)a;(void)b;(void)c;(void)e;}
static void repeat(void *d,struct zwp_input_method_keyboard_grab_v2 *g,int32_t r,int32_t delay){(void)d;(void)g;(void)r;(void)delay;}
static const struct zwp_input_method_keyboard_grab_v2_listener keys={keymap,key,modifiers,repeat};
int main(void){
 display=wl_display_connect(NULL);if(!display)return 1;
 struct wl_registry *registry=wl_display_get_registry(display);wl_registry_add_listener(registry,&registry_listener,NULL);
 if(wl_display_roundtrip(display)<0 || !seat || !manager)return 2;
 method=zwp_input_method_manager_v2_get_input_method(manager,seat);zwp_input_method_v2_add_listener(method,&listener,NULL);
 if(wl_display_roundtrip(display)<0 || unavailable)return 3;
 setvbuf(stdin,NULL,_IONBF,0);puts("ready");fflush(stdout);
 for(;;){
  while(wl_display_prepare_read(display)!=0)if(wl_display_dispatch_pending(display)<0)return 4;
  if(wl_display_flush(display)<0){wl_display_cancel_read(display);return 5;}
  struct pollfd fds[2]={{wl_display_get_fd(display),POLLIN,0},{STDIN_FILENO,POLLIN,0}};
  if(poll(fds,2,15000)<=0){wl_display_cancel_read(display);return 6;}
  if(fds[0].revents & POLLIN){if(wl_display_read_events(display)<0)return 7;if(wl_display_dispatch_pending(display)<0)return 8;}
  else wl_display_cancel_read(display);
  if(fds[1].revents & POLLIN){
   char line[1024];if(!fgets(line,sizeof(line),stdin))break;
   unsigned int id;char op,extra;
   if(sscanf(line,"%u %c %c",&id,&op,&extra)!=2 || !id)return 9;
   if(op=='p' || op=='g'){
    if(!active)return 10;
    if(op=='g' && !grab){grab=zwp_input_method_v2_grab_keyboard(method);zwp_input_method_keyboard_grab_v2_add_listener(grab,&keys,NULL);}
    composing=1;zwp_input_method_v2_set_preedit_string(method,"にほん",9,9);zwp_input_method_v2_commit(method,serial);
   }else if(op=='c'){zwp_input_method_v2_set_preedit_string(method,"",0,0);zwp_input_method_v2_commit_string(method,"café é 😀 日本語");zwp_input_method_v2_commit(method,serial);composing=0;}
   else if(op=='r'){if(grab){zwp_input_method_keyboard_grab_v2_release(grab);grab=NULL;}}
   else if(op=='x'){zwp_input_method_v2_set_preedit_string(method,"",0,0);zwp_input_method_v2_commit(method,serial);composing=0;}
   else if(op!='s')return 11;
   if(wl_display_roundtrip(display)<0)return 12;
   printf("ack %u %d\n",id,active);fflush(stdout);
  }else if(fds[1].revents & POLLHUP)break;
 }
 if(grab)zwp_input_method_keyboard_grab_v2_release(grab);
 zwp_input_method_v2_destroy(method);zwp_input_method_manager_v2_destroy(manager);wl_seat_destroy(seat);wl_registry_destroy(registry);wl_display_roundtrip(display);wl_display_disconnect(display);return 0;
}
