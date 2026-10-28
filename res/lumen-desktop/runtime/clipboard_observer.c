#define _POSIX_C_SOURCE 200809L
/* Exact-offer text transport. Consent, retention and storage live in Rust. */
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include "wlr-data-control-client-protocol.h"
#define MAX_BYTES (256u * 1024u)
struct offer { struct zwlr_data_control_offer_v1 *proxy; char *mime; bool sensitive; };
static struct wl_display *display;
static struct zwlr_data_control_manager_v1 *manager;
static struct wl_seat *seat;
static struct offer *pending;
static int input_fd = -1;
static unsigned char bytes[MAX_BYTES];
static size_t used;
static uint64_t deadline, generation, captured_ns;
static bool initial = true, running = true;
static uint64_t now_ms(void) { struct timespec t={0};if(clock_gettime(CLOCK_BOOTTIME,&t)<0){running=false;return 0;}return (uint64_t)t.tv_sec*1000+(uint64_t)t.tv_nsec/1000000; }
static uint64_t boot_ns(void) {struct timespec t={0};if(clock_gettime(CLOCK_BOOTTIME,&t)<0){running=false;return 0;}return (uint64_t)t.tv_sec*1000000000+(uint64_t)t.tv_nsec;}
static void drop_offer(struct offer *offer) { if (!offer)return;zwlr_data_control_offer_v1_destroy(offer->proxy);free(offer->mime);free(offer); }
static void cancel_transfer(void) { if(input_fd>=0)close(input_fd);input_fd=-1;drop_offer(pending);pending=NULL;memset(bytes,0,used);used=0;generation++; }
static bool write_all(const void *data,size_t size) {
 const unsigned char *p=data;
 while(size){ssize_t n=write(STDOUT_FILENO,p,size);if(n<0&&errno==EINTR)continue;if(n<=0)return false;p+=n;size-=(size_t)n;}
 return true;
}
/* Private stdout frames: kind byte, big-endian u32 length, payload.
 * Kind 1 is initial-event acknowledgement; kind 2 begins with BOOTTIME u64 ns. */
static bool frame(unsigned char kind,const void *data,size_t size) {
 unsigned char head[5]={kind,(unsigned char)(size>>24),(unsigned char)(size>>16),(unsigned char)(size>>8),(unsigned char)size};
 return write_all(head,sizeof(head))&&write_all(data,size);
}
static bool text_frame(void) {
 unsigned char timestamp[8];for(unsigned int i=0;i<8;i++)timestamp[7-i]=(unsigned char)(captured_ns>>(i*8));
 size_t size=used+8;unsigned char head[5]={2,(unsigned char)(size>>24),(unsigned char)(size>>16),(unsigned char)(size>>8),(unsigned char)size};
 return write_all(head,sizeof(head))&&write_all(timestamp,sizeof(timestamp))&&write_all(bytes,used);
}
static void mime_offer(void *data,struct zwlr_data_control_offer_v1 *proxy,const char *mime) {
 (void)proxy;struct offer *offer=data;
 if(!strcmp(mime,"x-kde-passwordManagerHint")||!strcmp(mime,"application/x-keepassxc"))offer->sensitive=true;
 if((!strcmp(mime,"text/plain;charset=utf-8")||!strcmp(mime,"text/plain"))&&(!offer->mime||!strcmp(mime,"text/plain;charset=utf-8"))){free(offer->mime);offer->mime=strdup(mime);if(!offer->mime)running=false;}
}
static const struct zwlr_data_control_offer_v1_listener offer_listener={mime_offer};
static void data_offer(void *data,struct zwlr_data_control_device_v1 *device,struct zwlr_data_control_offer_v1 *proxy) {
 (void)data;(void)device;struct offer *offer=calloc(1,sizeof(*offer));if(!offer){running=false;return;}
 offer->proxy=proxy;zwlr_data_control_offer_v1_set_user_data(proxy,offer);zwlr_data_control_offer_v1_add_listener(proxy,&offer_listener,offer);
}
static void selection(void *data,struct zwlr_data_control_device_v1 *device,struct zwlr_data_control_offer_v1 *proxy) {
 (void)data;(void)device;struct offer *offer=proxy?zwlr_data_control_offer_v1_get_user_data(proxy):NULL;
 cancel_transfer();
 /* data-control guarantees an initial selection event, including a null offer. */
 if(initial){initial=false;drop_offer(offer);running=frame(1,NULL,0);return;}
 if(!offer||offer->sensitive||!offer->mime){drop_offer(offer);return;}
 int fds[2];if(pipe(fds)<0){drop_offer(offer);running=false;return;}
 if(fcntl(fds[0],F_SETFL,O_NONBLOCK)<0){close(fds[0]);close(fds[1]);drop_offer(offer);running=false;return;}
 pending=offer;input_fd=fds[0];used=0;captured_ns=boot_ns();deadline=now_ms()+3000;
 zwlr_data_control_offer_v1_receive(proxy,offer->mime,fds[1]);close(fds[1]);
}
static void finished(void *data,struct zwlr_data_control_device_v1 *device){(void)data;(void)device;running=false;}
static void primary(void *data,struct zwlr_data_control_device_v1 *device,struct zwlr_data_control_offer_v1 *proxy){(void)data;(void)device;if(proxy)drop_offer(zwlr_data_control_offer_v1_get_user_data(proxy));}
static const struct zwlr_data_control_device_v1_listener device_listener={data_offer,selection,finished,primary};
static void global(void *data,struct wl_registry *registry,uint32_t name,const char *interface,uint32_t version){
 (void)data;if(!strcmp(interface,"zwlr_data_control_manager_v1"))manager=wl_registry_bind(registry,name,&zwlr_data_control_manager_v1_interface,version<2?version:2);
 else if(!strcmp(interface,"wl_seat")&&!seat)seat=wl_registry_bind(registry,name,&wl_seat_interface,1);
}
static void removed(void *data,struct wl_registry *registry,uint32_t name){(void)data;(void)registry;(void)name;}
static const struct wl_registry_listener registry_listener={global,removed};
static void read_transfer(void){
 while(input_fd>=0){
  unsigned char block[65536];ssize_t n=read(input_fd,block,sizeof(block));
  if(n<0&&(errno==EAGAIN||errno==EWOULDBLOCK))return;
  if(n<0&&errno==EINTR)continue;
  if(n==0){if(used)running=text_frame();cancel_transfer();return;}
  if(n<0||used+(size_t)n>MAX_BYTES){cancel_transfer();return;}
  memcpy(bytes+used,block,(size_t)n);used+=(size_t)n;
 }
}
int main(void){
 signal(SIGPIPE,SIG_IGN);display=wl_display_connect(NULL);if(!display)return 1;
 struct wl_registry *registry=wl_display_get_registry(display);wl_registry_add_listener(registry,&registry_listener,NULL);
 if(wl_display_roundtrip(display)<0||!manager||!seat)return 2;
 struct zwlr_data_control_device_v1 *device=zwlr_data_control_manager_v1_get_data_device(manager,seat);
 if(!device){zwlr_data_control_manager_v1_destroy(manager);wl_seat_destroy(seat);wl_registry_destroy(registry);wl_display_disconnect(display);return 3;}
 zwlr_data_control_device_v1_add_listener(device,&device_listener,NULL);
 while(running){
  while(wl_display_prepare_read(display)!=0){if(wl_display_dispatch_pending(display)<0){running=false;break;}}
  if(!running)break;
  int flushed=wl_display_flush(display);if(flushed<0&&errno!=EAGAIN){wl_display_cancel_read(display);break;}
  struct pollfd fds[2]={{wl_display_get_fd(display),POLLIN|(flushed<0?POLLOUT:0),0},{input_fd,POLLIN,0}};
  uint64_t observed=generation,clock=now_ms();int timeout=input_fd<0?-1:(deadline>clock?(int)(deadline-clock):0);
  int ready=poll(fds,2,timeout);
  if(ready<0){wl_display_cancel_read(display);if(errno==EINTR)continue;break;}
  if(fds[0].revents&POLLIN){if(wl_display_read_events(display)<0)break;}else wl_display_cancel_read(display);
  if(fds[0].revents&(POLLERR|POLLHUP|POLLNVAL))break;
  if(wl_display_dispatch_pending(display)<0)break;
  if(input_fd>=0&&now_ms()>=deadline){cancel_transfer();continue;}
  if(input_fd>=0&&observed==generation&&fds[1].revents&(POLLIN|POLLHUP|POLLERR))read_transfer();
 }
 cancel_transfer();zwlr_data_control_device_v1_destroy(device);zwlr_data_control_manager_v1_destroy(manager);wl_seat_destroy(seat);wl_registry_destroy(registry);wl_display_disconnect(display);return 1;
}
