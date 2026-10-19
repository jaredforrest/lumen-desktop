/* Disposable protocol client for isolated compositor regression tests. */
#define _GNU_SOURCE
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>
#include "xdg-shell-client-protocol.h"
#include "pointer-constraints-unstable-v1-client-protocol.h"
static struct wl_display *display;
static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct wl_seat *seat;
struct seat_entry {struct wl_seat *seat;uint32_t capabilities;};
static struct seat_entry seats[16];
static int seat_count;
static void capabilities(void*d,struct wl_seat*s,uint32_t c){(void)s;((struct seat_entry*)d)->capabilities=c;}
static const struct wl_seat_listener seat_listener={.capabilities=capabilities};
static struct xdg_wm_base *wm;
static struct zwp_pointer_constraints_v1 *constraints;
static struct wl_surface *surface;
static struct wl_buffer *buffer;
static struct zwp_locked_pointer_v1 *locked_pointer;
static struct zwp_confined_pointer_v1 *confined_pointer;
static void ping(void *d,struct xdg_wm_base *w,uint32_t serial){(void)d;xdg_wm_base_pong(w,serial);}
static const struct xdg_wm_base_listener wm_listener={ping};
static void global(void *d,struct wl_registry *r,uint32_t n,const char *i,uint32_t v){
 (void)d;(void)v;
 if(!strcmp(i,"wl_compositor"))compositor=wl_registry_bind(r,n,&wl_compositor_interface,4);
 else if(!strcmp(i,"wl_shm"))shm=wl_registry_bind(r,n,&wl_shm_interface,1);
 else if(!strcmp(i,"wl_seat")&&seat_count<16){struct seat_entry *entry=&seats[seat_count++];entry->seat=wl_registry_bind(r,n,&wl_seat_interface,1);wl_seat_add_listener(entry->seat,&seat_listener,entry);}
 else if(!strcmp(i,"xdg_wm_base")){wm=wl_registry_bind(r,n,&xdg_wm_base_interface,1);xdg_wm_base_add_listener(wm,&wm_listener,NULL);}
 else if(!strcmp(i,"zwp_pointer_constraints_v1"))constraints=wl_registry_bind(r,n,&zwp_pointer_constraints_v1_interface,1);
}
static void removed(void*d,struct wl_registry*r,uint32_t n){(void)d;(void)r;(void)n;}
static const struct wl_registry_listener registry_listener={global,removed};
static void configured(void*d,struct xdg_surface*s,uint32_t serial){
 struct wl_surface *target=d?d:surface;xdg_surface_ack_configure(s,serial);wl_surface_attach(target,buffer,0,0);wl_surface_damage(target,0,0,400,300);wl_surface_commit(target);puts("mapped");fflush(stdout);
}
static const struct xdg_surface_listener surface_listener={configured};
static void active_lock(void*d,struct zwp_locked_pointer_v1*p){(void)d;(void)p;puts("locked");fflush(stdout);}
static void inactive_lock(void*d,struct zwp_locked_pointer_v1*p){(void)d;(void)p;puts("unlocked");fflush(stdout);}
static const struct zwp_locked_pointer_v1_listener lock_listener={active_lock,inactive_lock};
static void active_confine(void*d,struct zwp_confined_pointer_v1*p){(void)d;(void)p;puts("confined");fflush(stdout);}
static void inactive_confine(void*d,struct zwp_confined_pointer_v1*p){(void)d;(void)p;puts("unconfined");fflush(stdout);}
static const struct zwp_confined_pointer_v1_listener confine_listener={active_confine,inactive_confine};
static void top_configure(void*d,struct xdg_toplevel*t,int32_t w,int32_t h,struct wl_array*states){(void)d;(void)t;(void)w;(void)h;(void)states;}
static void top_close(void*d,struct xdg_toplevel*t){(void)d;(void)t;}
static const struct xdg_toplevel_listener top_listener={.configure=top_configure,.close=top_close};
static void pointer_enter(void*d,struct wl_pointer*p,uint32_t s,struct wl_surface*w,wl_fixed_t x,wl_fixed_t y){(void)d;(void)p;(void)s;(void)w;printf("enter %.2f %.2f\n",wl_fixed_to_double(x),wl_fixed_to_double(y));fflush(stdout);}
static void pointer_leave(void*d,struct wl_pointer*p,uint32_t s,struct wl_surface*w){(void)d;(void)p;(void)s;(void)w;}
static void pointer_motion(void*d,struct wl_pointer*p,uint32_t t,wl_fixed_t x,wl_fixed_t y){(void)d;(void)p;(void)t;printf("motion %.2f %.2f\n",wl_fixed_to_double(x),wl_fixed_to_double(y));fflush(stdout);}
static void pointer_button(void*d,struct wl_pointer*p,uint32_t s,uint32_t t,uint32_t b,uint32_t v){(void)d;(void)p;(void)s;(void)t;(void)b;(void)v;}
static void pointer_axis(void*d,struct wl_pointer*p,uint32_t t,uint32_t a,wl_fixed_t v){(void)d;(void)p;(void)t;(void)a;(void)v;}
static const struct wl_pointer_listener pointer_listener={.enter=pointer_enter,.leave=pointer_leave,.motion=pointer_motion,.button=pointer_button,.axis=pointer_axis};
int main(int argc,char **argv){
 display=wl_display_connect(NULL);if(!display)return 1;
 struct wl_registry *registry=wl_display_get_registry(display);wl_registry_add_listener(registry,&registry_listener,NULL);wl_display_roundtrip(display);
 if(!compositor||!shm||!seat_count||!wm||!constraints)return 2;
 wl_display_roundtrip(display);for(int i=0;i<seat_count;i++)if(seats[i].capabilities&WL_SEAT_CAPABILITY_POINTER){seat=seats[i].seat;break;}
 if(!seat){fputs("no pointer capability\n",stderr);return 5;}
 struct wl_pointer *pointer=wl_seat_get_pointer(seat);wl_pointer_add_listener(pointer,&pointer_listener,NULL);
 int fd=memfd_create("lumen-constraint-fixture",MFD_CLOEXEC);if(fd<0||ftruncate(fd,400*300*4))return 3;
 void *pixels=mmap(NULL,400*300*4,PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);if(pixels==MAP_FAILED)return 4;
 for(int i=0;i<400*300;i++)((uint32_t*)pixels)[i]=0xff303840;
 struct wl_shm_pool *pool=wl_shm_create_pool(shm,fd,400*300*4);buffer=wl_shm_pool_create_buffer(pool,0,400,300,400*4,WL_SHM_FORMAT_XRGB8888);wl_shm_pool_destroy(pool);close(fd);
 surface=wl_compositor_create_surface(compositor);struct xdg_surface *xdg=xdg_wm_base_get_xdg_surface(wm,surface);xdg_surface_add_listener(xdg,&surface_listener,NULL);
 struct xdg_toplevel *top=xdg_surface_get_toplevel(xdg);xdg_toplevel_add_listener(top,&top_listener,NULL);xdg_toplevel_set_app_id(top,"constraint-test");xdg_toplevel_set_title(top,"Constraint fixture");
 if(argc>1&&!strcmp(argv[1],"fixed")){xdg_toplevel_set_min_size(top,400,300);xdg_toplevel_set_max_size(top,400,300);}
 if(argc>1&&!strcmp(argv[1],"bounded")){xdg_toplevel_set_min_size(top,200,100);xdg_toplevel_set_max_size(top,500,1000);}
 wl_surface_commit(surface);wl_display_roundtrip(display);
 struct wl_surface *dialog=NULL;struct xdg_surface *dialog_xdg=NULL;struct xdg_toplevel *dialog_top=NULL;
 struct pollfd fds[2]={{wl_display_get_fd(display),POLLIN,0},{STDIN_FILENO,POLLIN,0}};
 for(;;){
  wl_display_dispatch_pending(display);wl_display_flush(display);if(poll(fds,2,-1)<0)break;
  if(fds[0].revents&POLLIN)if(wl_display_dispatch(display)<0)break;
  if(fds[1].revents&(POLLIN|POLLHUP)){
   char c;if(read(STDIN_FILENO,&c,1)!=1)break;
   if(c=='l'){locked_pointer=zwp_pointer_constraints_v1_lock_pointer(constraints,surface,pointer,NULL,ZWP_POINTER_CONSTRAINTS_V1_LIFETIME_PERSISTENT);zwp_locked_pointer_v1_add_listener(locked_pointer,&lock_listener,NULL);wl_surface_commit(surface);}
   if(c=='c'){confined_pointer=zwp_pointer_constraints_v1_confine_pointer(constraints,surface,pointer,NULL,ZWP_POINTER_CONSTRAINTS_V1_LIFETIME_PERSISTENT);zwp_confined_pointer_v1_add_listener(confined_pointer,&confine_listener,NULL);wl_surface_commit(surface);}
   if(c=='u'){if(locked_pointer){zwp_locked_pointer_v1_destroy(locked_pointer);locked_pointer=NULL;}if(confined_pointer){zwp_confined_pointer_v1_destroy(confined_pointer);confined_pointer=NULL;}wl_surface_commit(surface);}
   if((c=='t'||c=='f')&&top&&!dialog){dialog=wl_compositor_create_surface(compositor);dialog_xdg=xdg_wm_base_get_xdg_surface(wm,dialog);xdg_surface_add_listener(dialog_xdg,&surface_listener,dialog);dialog_top=xdg_surface_get_toplevel(dialog_xdg);xdg_toplevel_add_listener(dialog_top,&top_listener,NULL);xdg_toplevel_set_parent(dialog_top,top);xdg_toplevel_set_app_id(dialog_top,"constraint-dialog");xdg_toplevel_set_title(dialog_top,"Transient fixture");if(c=='t'){xdg_toplevel_set_min_size(dialog_top,200,150);xdg_toplevel_set_max_size(dialog_top,200,150);}xdg_surface_set_window_geometry(dialog_xdg,0,0,200,150);wl_surface_commit(dialog);}
   if(c=='d'&&dialog){xdg_toplevel_destroy(dialog_top);xdg_surface_destroy(dialog_xdg);wl_surface_destroy(dialog);dialog=NULL;puts("dialog-closed");fflush(stdout);}
   if(c=='n'&&dialog){xdg_toplevel_set_parent(dialog_top,NULL);wl_surface_commit(dialog);puts("dialog-independent");fflush(stdout);}
   if(c=='p'&&top){xdg_toplevel_destroy(top);top=NULL;xdg_surface_destroy(xdg);wl_surface_destroy(surface);surface=NULL;puts("parent-closed");fflush(stdout);}
   if(c=='q')break;
  }
 }
 wl_display_disconnect(display);munmap(pixels,400*300*4);return 0;
}
