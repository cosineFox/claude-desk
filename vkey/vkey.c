// vkey: virtual keyboard with the standard US keymap so XWayland's raw keycodes match.
// build: cc -O2 -o vkey vkey.c vk.c -lwayland-client -lxkbcommon
// t:TEXT types US-keymap ASCII (newline = Return, tab = Tab; other characters are skipped with a message)
// usage: WAYLAND_DISPLAY=wayland-1 vkey TOKEN...   TOKEN = keysym | mod+...+keysym | t:text
#define _GNU_SOURCE
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>
#include <sys/mman.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <time.h>
#include "vk.h"
static struct wl_seat *seat; static struct zwp_virtual_keyboard_manager_v1 *mgr;
static void g(void *d, struct wl_registry *r, uint32_t n, const char *i, uint32_t v){
 if(!strcmp(i,"wl_seat")&&!seat) seat=wl_registry_bind(r,n,&wl_seat_interface,1);
 if(!strcmp(i,"zwp_virtual_keyboard_manager_v1")) mgr=wl_registry_bind(r,n,&zwp_virtual_keyboard_manager_v1_interface,1);}
static void gr(void*d,struct wl_registry*r,uint32_t n){}
static const struct wl_registry_listener rl={g,gr};
static struct xkb_keymap *km; static struct zwp_virtual_keyboard_v1 *kb; static struct wl_display *dp;
static uint32_t ms(){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1000+t.tv_nsec/1000000;}
static int find(xkb_keysym_t s,int *shift){
 for(xkb_keycode_t k=8;k<256;k++) for(int l=0;l<2;l++){const xkb_keysym_t*ss;
  int n=xkb_keymap_key_get_syms_by_level(km,k,0,l,&ss); if(n>0&&ss[0]==s){*shift=l;return k-8;}}
 return -1;}
static uint32_t modmask;
static void ev(int code,int down){zwp_virtual_keyboard_v1_key(kb,ms(),code,down);wl_display_roundtrip(dp);usleep(30000);}
static int modcode(const char*m,uint32_t*mask){
 if(!strcmp(m,"ctrl")){*mask=4;return 29;} if(!strcmp(m,"alt")){*mask=8;return 56;}
 if(!strcmp(m,"shift")){*mask=1;return 42;} if(!strcmp(m,"super")){*mask=64;return 125;} return -1;}
static void setmods(){zwp_virtual_keyboard_v1_modifiers(kb,modmask,0,0,0);}
static void press(xkb_keysym_t s,char mods[][16],int nm){
 int sh; int c=find(s,&sh); if(c<0){fprintf(stderr,"no key for sym %u\n",s);return;}
 int mc[8];uint32_t mm[8];
 for(int i=0;i<nm;i++){mc[i]=modcode(mods[i],&mm[i]); if(mc[i]<0){fprintf(stderr,"bad mod %s\n",mods[i]);return;}}
 for(int i=0;i<nm;i++){ev(mc[i],1);modmask|=mm[i];setmods();}
 if(sh){ev(42,1);modmask|=1;setmods();}
 ev(c,1);ev(c,0);
 if(sh){ev(42,0);modmask&=~1u;setmods();}
 for(int i=nm-1;i>=0;i--){ev(mc[i],0);modmask&=~mm[i];setmods();}}
int main(int argc,char**argv){
 const char*w=getenv("WAYLAND_DISPLAY"); if(!w||!strcmp(w,"wayland-0")){fprintf(stderr,"refusing: set WAYLAND_DISPLAY to the desk\n");return 2;}
 dp=wl_display_connect(NULL); if(!dp){fprintf(stderr,"no display\n");return 1;}
 struct wl_registry*r=wl_display_get_registry(dp); wl_registry_add_listener(r,&rl,0); wl_display_roundtrip(dp);
 if(!seat||!mgr){fprintf(stderr,"no seat/vk manager\n");return 1;}
 struct xkb_context*cx=xkb_context_new(0); struct xkb_rule_names nm={"evdev","pc105","us","",""};
 km=xkb_keymap_new_from_names(cx,&nm,0); char*str=xkb_keymap_get_as_string(km,XKB_KEYMAP_FORMAT_TEXT_V1);
 size_t len=strlen(str)+1; int fd=memfd_create("km",0); write(fd,str,len);
 kb=zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(mgr,seat);
 zwp_virtual_keyboard_v1_keymap(kb,WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1,fd,len); wl_display_roundtrip(dp); usleep(50000);
 for(int a=1;a<argc;a++){
  if(!strncmp(argv[a],"down:",5)||!strncmp(argv[a],"up:",3)){int dn=argv[a][0]=='d';uint32_t m;int c=modcode(argv[a]+(dn?5:3),&m);
    if(c<0)continue; ev(c,dn); if(dn)modmask|=m; else modmask&=~m; setmods(); continue;}
  if(!strncmp(argv[a],"sleep:",6)){usleep(atoi(argv[a]+6)*1000);continue;}
  if(!strncmp(argv[a],"t:",2)){ for(char*p=argv[a]+2;*p;p++){ xkb_keysym_t ks=*p=='\n'?XKB_KEY_Return:*p=='\t'?XKB_KEY_Tab:xkb_utf32_to_keysym((unsigned char)*p); press(ks,NULL,0);} continue;}
  char buf[128];strncpy(buf,argv[a],127);buf[127]=0; char mods[8][16];int n=0; char*tok=buf,*plus;
  while((plus=strchr(tok,'+'))&&plus[1]){*plus=0;strncpy(mods[n++],tok,15);mods[n-1][15]=0;tok=plus+1;}
  xkb_keysym_t s=xkb_keysym_from_name(tok,XKB_KEYSYM_NO_FLAGS); if(!s) s=xkb_keysym_from_name(tok,XKB_KEYSYM_CASE_INSENSITIVE);
  press(s,mods,n); usleep(60000);}
 wl_display_roundtrip(dp); return 0;}
