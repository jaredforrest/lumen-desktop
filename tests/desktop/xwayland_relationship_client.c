/* Disposable X11 protocol client: no toolkit behavior or app identity inference. */
#include <xcb/xcb.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <unistd.h>

#define WINDOWS 4
static xcb_connection_t *connection;
static xcb_screen_t *screen;
static xcb_window_t windows[WINDOWS];
static unsigned keys[WINDOWS];
static xcb_atom_t transient, state, modal, pid_hint, protocols, delete_window;
static unsigned serial;
static int xres;
static xcb_atom_t atom(const char *name) {
 xcb_intern_atom_reply_t *reply=xcb_intern_atom_reply(connection,
  xcb_intern_atom(connection,0,(uint16_t)strlen(name),name),NULL);
 if(!reply)exit(2);
 xcb_atom_t value=reply->atom;free(reply);return value;
}
static void status(void) {
 printf("{\"serial\":%u,\"pid\":%ld,\"xres\":%s,\"windows\":[",serial,(long)getpid(),xres?"true":"false");
 for(int i=0;i<WINDOWS;i++) {
  xcb_get_window_attributes_reply_t *reply=windows[i]?xcb_get_window_attributes_reply(connection,xcb_get_window_attributes(connection,windows[i]),NULL):NULL;
  printf("%s{\"id\":%u,\"keys\":%u,\"mapState\":%d,\"overrideRedirect\":%d}",i?",":"",windows[i],keys[i],reply?reply->map_state:-1,reply?reply->override_redirect:-1);free(reply);
 }
 puts("]}");fflush(stdout);
}
static void create(int index,const char *label) {
 windows[index]=xcb_generate_id(connection);
 uint32_t values[]={screen->white_pixel,XCB_EVENT_MASK_EXPOSURE|XCB_EVENT_MASK_KEY_PRESS|XCB_EVENT_MASK_STRUCTURE_NOTIFY};
 xcb_create_window(connection,XCB_COPY_FROM_PARENT,windows[index],screen->root,
  (int16_t)(80+index*40),(int16_t)(100+index*35),320,220,0,XCB_WINDOW_CLASS_INPUT_OUTPUT,
  screen->root_visual,XCB_CW_BACK_PIXEL|XCB_CW_EVENT_MASK,values);
 char title[160];snprintf(title,sizeof(title),"Synthetic X11 %s %d",label,index);
 xcb_change_property(connection,XCB_PROP_MODE_REPLACE,windows[index],XCB_ATOM_WM_NAME,XCB_ATOM_STRING,8,(uint32_t)strlen(title),title);
 const char klass[]="fixture\0org.example.Relationships\0";
 xcb_change_property(connection,XCB_PROP_MODE_REPLACE,windows[index],XCB_ATOM_WM_CLASS,XCB_ATOM_STRING,8,sizeof(klass)-1,klass);
 xcb_change_property(connection,XCB_PROP_MODE_REPLACE,windows[index],protocols,XCB_ATOM_ATOM,32,1,&delete_window);
}
int main(int argc,char **argv) {
 if(argc!=2 || strlen(argv[1])>80)return 2;
 connection=xcb_connect(NULL,NULL);if(xcb_connection_has_error(connection))return 2;
 screen=xcb_setup_roots_iterator(xcb_get_setup(connection)).data;
 const char extension[]="X-Resource";
 xcb_query_extension_reply_t *reply=xcb_query_extension_reply(connection,xcb_query_extension(connection,sizeof(extension)-1,extension),NULL);xres=reply && reply->present;free(reply);
 transient=atom("WM_TRANSIENT_FOR");state=atom("_NET_WM_STATE");modal=atom("_NET_WM_STATE_MODAL");pid_hint=atom("_NET_WM_PID");protocols=atom("WM_PROTOCOLS");delete_window=atom("WM_DELETE_WINDOW");
 for(int i=0;i<WINDOWS;i++)create(i,argv[1]);
 xcb_flush(connection);status();
 char line[256];
 for(;;) {
  xcb_generic_event_t *event;
  while((event=xcb_poll_for_event(connection))) {
   unsigned kind=event->response_type&127;
   if(kind==XCB_KEY_PRESS) {
    xcb_key_press_event_t *key=(xcb_key_press_event_t*)event;
    for(int i=0;i<WINDOWS;i++)if(windows[i]==key->event)keys[i]++;
    status();
   } else if(kind==XCB_CLIENT_MESSAGE) {
    xcb_client_message_event_t *message=(xcb_client_message_event_t*)event;
    if(message->type==protocols && message->data.data32[0]==delete_window) {
     for(int i=0;i<WINDOWS;i++)if(windows[i]==message->window){xcb_destroy_window(connection,windows[i]);windows[i]=0;}
     xcb_flush(connection);status();
    }
   }
   free(event);
  }
  if(xcb_connection_has_error(connection))break;
  fd_set descriptors;FD_ZERO(&descriptors);FD_SET(STDIN_FILENO,&descriptors);
  int fd=xcb_get_file_descriptor(connection);FD_SET(fd,&descriptors);
  if(select(fd+1,&descriptors,NULL,NULL,NULL)<0){if(errno==EINTR)continue;break;}
  if(!FD_ISSET(STDIN_FILENO,&descriptors))continue;
  if(!fgets(line,sizeof(line),stdin))break;
  char command[32];unsigned index=0,value=0;
  if(sscanf(line,"%31s %u %u",command,&index,&value)<1 || index>=WINDOWS)break;
  if(strcmp(command,"exit")==0)break;
  if(strcmp(command,"new")==0){if(windows[index])break;create((int)index,argv[1]);}
  else if(!windows[index])break;
  else if(strcmp(command,"map")==0)xcb_map_window(connection,windows[index]);
  else if(strcmp(command,"unmap")==0)xcb_unmap_window(connection,windows[index]);
  else if(strcmp(command,"destroy")==0){xcb_destroy_window(connection,windows[index]);windows[index]=0;}
  else if(strcmp(command,"parent")==0) {
   if(value)xcb_change_property(connection,XCB_PROP_MODE_REPLACE,windows[index],transient,XCB_ATOM_WINDOW,32,1,&value);
   else xcb_delete_property(connection,windows[index],transient);
  } else if(strcmp(command,"pid")==0)xcb_change_property(connection,XCB_PROP_MODE_REPLACE,windows[index],pid_hint,XCB_ATOM_CARDINAL,32,1,&value);
  else if(strcmp(command,"override")==0) {
   xcb_change_window_attributes(connection,windows[index],XCB_CW_OVERRIDE_REDIRECT,&value);
   /* XWM observes the override flag through an actual ConfigureNotify. */
   uint32_t x=80+index*40+(value?1:0);xcb_configure_window(connection,windows[index],XCB_CONFIG_WINDOW_X,&x);
  }
  else if(strcmp(command,"property")==0) {
   if(value)xcb_change_property(connection,XCB_PROP_MODE_REPLACE,windows[index],state,XCB_ATOM_ATOM,32,1,&modal);
   else xcb_delete_property(connection,windows[index],state);
  } else if(strcmp(command,"modal")==0) {
   xcb_client_message_event_t message={0};message.response_type=XCB_CLIENT_MESSAGE;message.format=32;message.window=windows[index];message.type=state;
   message.data.data32[0]=value?1:0;message.data.data32[1]=modal;message.data.data32[3]=1;
   xcb_send_event(connection,0,screen->root,XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT|XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY,(const char*)&message);
  } else break;
  xcb_flush(connection);
  xcb_get_input_focus_reply_t *ack=xcb_get_input_focus_reply(connection,xcb_get_input_focus(connection),NULL);free(ack);
  serial++;status();
 }
 xcb_disconnect(connection);return 0;
}
