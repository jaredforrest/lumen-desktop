"""Provider startup cannot select the retained application's presentation policy."""
KEYS=('FONTCONFIG_FILE','XDG_CURRENT_DESKTOP','GTK_USE_PORTAL')

def start_preserving_client(start,environment,*arguments):
 saved={key:environment.get(key) for key in KEYS}
 try:return start(environment,*arguments)
 finally:
  for key,value in saved.items():
   if value is None:environment.pop(key,None)
   else:environment[key]=value
