/* Disposable native parent/dialog and keyboard-recipient fixture. */
#define _GNU_SOURCE
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>
#include "xdg-shell-client-protocol.h"
#include "xdg-dialog-v1-client-protocol.h"
static struct wl_display *display;
static struct wl_compositor *compositor;
static struct wl_shm *shm;
static struct xdg_wm_base *wm;
static struct xdg_wm_dialog_v1 *dialogs;
static struct wl_keyboard *keyboard;
static struct wl_buffer *buffer;
struct window {struct wl_surface *surface;struct xdg_surface *xdg;struct xdg_toplevel *top;struct xdg_dialog_v1 *dialog;int id;};
static struct window windows[4];
static int key_window=-1;
static void keymap(void*d,struct wl_keyboard*k,uint32_t f,int32_t fd,uint32_t size){(void)d;(void)k;(void)f;(void)size;close(fd);}
static void enter(void*d,struct wl_keyboard*k,uint32_t serial,struct wl_surface*s,struct wl_array*keys){(void)d;(void)k;(void)serial;(void)keys;key_window=-1;for(int i=0;i<4;i++)if(windows[i].surface==s)key_window=i;printf("enter %d\n",key_window);fflush(stdout);}
static void leave(void*d,struct wl_keyboard*k,uint32_t serial,struct wl_surface*s){(void)d;(void)k;(void)serial;(void)s;key_window=-1;}
static void key(void*d,struct wl_keyboard*k,uint32_t serial,uint32_t time,uint32_t code,uint32_t state){(void)d;(void)k;(void)serial;(void)time;printf("key %d %u %u\n",key_window,code,state);fflush(stdout);}
static void modifiers(void*d,struct wl_keyboard*k,uint32_t serial,uint32_t a,uint32_t b,uint32_t c,uint32_t group){(void)d;(void)k;(void)serial;(void)a;(void)b;(void)c;(void)group;}
static void repeat(void*d,struct wl_keyboard*k,int32_t rate,int32_t delay){(void)d;(void)k;(void)rate;(void)delay;}
static const struct wl_keyboard_listener keyboard_listener={keymap,enter,leave,key,modifiers,repeat};
static void capabilities(void*d,struct wl_seat*s,uint32_t caps){(void)d;if((caps&WL_SEAT_CAPABILITY_KEYBOARD)&&!keyboard){keyboard=wl_seat_get_keyboard(s);wl_keyboard_add_listener(keyboard,&keyboard_listener,NULL);}}
static void seat_name(void*d,struct wl_seat*s,const char*n){(void)d;(void)s;(void)n;}
static const struct wl_seat_listener seat_listener={capabilities,seat_name};
static void ping(void*d,struct xdg_wm_base*w,uint32_t serial){(void)d;xdg_wm_base_pong(w,serial);}
static const struct xdg_wm_base_listener wm_listener={ping};
static void global(void*d,struct wl_registry*r,uint32_t n,const char*i,uint32_t v){(void)d;(void)v;
 if(!strcmp(i,"wl_compositor"))compositor=wl_registry_bind(r,n,&wl_compositor_interface,4);
 else if(!strcmp(i,"wl_shm"))shm=wl_registry_bind(r,n,&wl_shm_interface,1);
 else if(!strcmp(i,"wl_seat")){struct wl_seat*s=wl_registry_bind(r,n,&wl_seat_interface,5);wl_seat_add_listener(s,&seat_listener,NULL);}
 else if(!strcmp(i,"xdg_wm_base")){wm=wl_registry_bind(r,n,&xdg_wm_base_interface,1);xdg_wm_base_add_listener(wm,&wm_listener,NULL);}
 else if(!strcmp(i,"xdg_wm_dialog_v1"))dialogs=wl_registry_bind(r,n,&xdg_wm_dialog_v1_interface,1);
}
static void removed(void*d,struct wl_registry*r,uint32_t n){(void)d;(void)r;(void)n;}
static const struct wl_registry_listener registry_listener={global,removed};
static void configured(void*d,struct xdg_surface*s,uint32_t serial){struct window*w=d;xdg_surface_ack_configure(s,serial);wl_surface_attach(w->surface,buffer,0,0);wl_surface_damage(w->surface,0,0,320,240);wl_surface_commit(w->surface);printf("mapped %d\n",w->id);fflush(stdout);}
static const struct xdg_surface_listener surface_listener={configured};
static void top_configure(void*d,struct xdg_toplevel*t,int32_t width,int32_t height,struct wl_array*states){(void)d;(void)t;(void)width;(void)height;(void)states;}
static void top_close(void*d,struct xdg_toplevel*t){(void)d;(void)t;}
static const struct xdg_toplevel_listener top_listener={.configure=top_configure,.close=top_close};
static void destroy_window(int n){struct window*w=&windows[n];if(w->dialog){xdg_dialog_v1_destroy(w->dialog);w->dialog=NULL;}xdg_toplevel_destroy(w->top);xdg_surface_destroy(w->xdg);wl_surface_destroy(w->surface);w->surface=NULL;w->top=NULL;}
int main(void){
 display=wl_display_connect(NULL);if(!display)return 1;struct wl_registry*r=wl_display_get_registry(display);wl_registry_add_listener(r,&registry_listener,NULL);wl_display_roundtrip(display);
 if(!compositor||!shm||!wm||!dialogs)return 2;
 int fd=memfd_create("relationship-fixture",MFD_CLOEXEC);size_t size=320*240*4;if(fd<0||ftruncate(fd,(off_t)size))return 3;void*data=mmap(NULL,size,PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);if(data==MAP_FAILED)return 4;for(size_t i=0;i<size/4;i++)((uint32_t*)data)[i]=0xff597a91;
 struct wl_shm_pool*pool=wl_shm_create_pool(shm,fd,(int)size);buffer=wl_shm_pool_create_buffer(pool,0,320,240,320*4,WL_SHM_FORMAT_ARGB8888);wl_shm_pool_destroy(pool);close(fd);munmap(data,size);
 for(int i=0;i<4;i++){struct window*w=&windows[i];w->id=i;w->surface=wl_compositor_create_surface(compositor);w->xdg=xdg_wm_base_get_xdg_surface(wm,w->surface);xdg_surface_add_listener(w->xdg,&surface_listener,w);w->top=xdg_surface_get_toplevel(w->xdg);xdg_toplevel_add_listener(w->top,&top_listener,w);char title[32];snprintf(title,sizeof(title),"Relationship %d",i);xdg_toplevel_set_title(w->top,title);xdg_toplevel_set_app_id(w->top,i==3?"org.example.Other":i==2?"org.example.Utility":"org.example.Editor");if(i==2)xdg_toplevel_set_parent(w->top,windows[0].top);wl_surface_commit(w->surface);}
 wl_display_roundtrip(display);wl_display_roundtrip(display);xdg_toplevel_set_parent(windows[2].top,windows[0].top);wl_display_roundtrip(display);puts("ready");fflush(stdout);
 while(1){if(wl_display_dispatch_pending(display)<0)return 5;wl_display_flush(display);struct pollfd fds[2]={{wl_display_get_fd(display),POLLIN,0},{STDIN_FILENO,POLLIN,0}};if(poll(fds,2,-1)<0)return 6;if(fds[0].revents&POLLIN){if(wl_display_dispatch(display)<0)return 7;}if(fds[1].revents&POLLIN){char line[80];if(!fgets(line,sizeof(line),stdin))break;if(!strncmp(line,"modal",5)){if(!windows[2].dialog)windows[2].dialog=xdg_wm_dialog_v1_get_xdg_dialog(dialogs,windows[2].top);xdg_dialog_v1_set_modal(windows[2].dialog);}else if(!strncmp(line,"unmodal",7)){xdg_dialog_v1_unset_modal(windows[2].dialog);}else if(!strncmp(line,"unparent",8)){xdg_toplevel_set_parent(windows[2].top,NULL);}else if(!strncmp(line,"destroy-dialog",14)){xdg_dialog_v1_destroy(windows[2].dialog);windows[2].dialog=NULL;}else if(!strncmp(line,"reparent",8)){xdg_toplevel_set_parent(windows[2].top,windows[1].top);}else if(!strncmp(line,"close-second",12)){destroy_window(1);}else if(!strncmp(line,"close-parent",12)){destroy_window(0);}else if(!strncmp(line,"close-panel",11)){destroy_window(2);}else if(!strncmp(line,"quit",4))break;wl_display_roundtrip(display);printf("ack %s",line);fflush(stdout);}}
 wl_display_disconnect(display);return 0;
}
