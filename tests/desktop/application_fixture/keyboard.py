"""Stable full-US keyboard events for actual isolated application shortcuts."""
import select,subprocess,time
MODIFIERS={'shift':1,'ctrl':4,'alt':8,'logo':64}
SPECIAL={'Escape':1,'Return':28,'Tab':15,'BackSpace':14,'Delete':111,'Left':105,'Right':106,'Up':103,'Down':108,'Home':102,'End':107,'F11':87,'F12':88,'space':57,'minus':12}
LETTERS={letter:code for letters,start in [('qwertyuiop',16),('asdfghjkl',30),('zxcvbnm',44)] for code,letter in enumerate(letters,start)}
# Use physical evdev codes, not a replacement per-command keymap.
PLAIN={**dict(zip('1234567890',range(2,12))),'-':12,'=':13,'[':26,']':27,';':39,"'":40,'`':41,'\\':43,',':51,'.':52,'/':53,' ':57,**LETTERS}
SHIFTED=dict(zip('!@#$%^&*()_+{}:"~|<>?','1234567890-=[];\'`\\,./'))

def text_events(value):
 result=[]
 for char in value:
  if char in PLAIN:code,mods=PLAIN[char],0
  elif char in SHIFTED:code,mods=PLAIN[SHIFTED[char]],1
  elif char.isascii() and char.isupper():code,mods=LETTERS[char.lower()],1
  else:raise ValueError('Use the private clipboard for non-US text; no layout guessing')
  result.extend([('m',mods,0),('k',code,1),('k',code,0)])
 result.append(('m',0,0));return result

class PhysicalKeyboard:
 def __init__(self,binary,env,log):
  self.process=subprocess.Popen([str(binary)],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,bufsize=1,start_new_session=True);self.sequence=0;self.modifiers=0
  if not select.select([self.process.stdout],[],[],3)[0] or self.process.stdout.readline().strip()!='ready':raise RuntimeError('Stable private keyboard did not become ready')
 def send(self,operation,value,state=0):
  self.sequence+=1;self.process.stdin.write('%s %d %d %d\n'%(operation,self.sequence,value,state));self.process.stdin.flush()
  if not select.select([self.process.stdout],[],[],3)[0] or self.process.stdout.readline().strip()!='ack '+str(self.sequence):raise RuntimeError('Stable private keyboard did not acknowledge exact command')
 def type(self,value):
  for event in text_events(value):self.send(*event)
 def key(self,*values):
  if len(values)==1:self.type(values[0]);return
  index=0
  while index<len(values):
   operation,name=values[index:index+2];index+=2
   if operation in ('-M','-m'):
    bit=MODIFIERS[name];self.modifiers=self.modifiers|bit if operation=='-M' else self.modifiers&~bit;self.send('m',self.modifiers)
   elif operation=='-k':
    code=SPECIAL.get(name,PLAIN.get(name));
    if code is None:raise ValueError('Unsupported named physical key')
    self.send('k',code,1);self.send('k',code,0)
   else:raise ValueError('Unsupported structured keyboard operation')
  time.sleep(.12)
 def close(self):
  if self.process.stdin and not self.process.stdin.closed:self.process.stdin.close()
  self.process.wait(timeout=3)
  if self.process.stdout and not self.process.stdout.closed:self.process.stdout.close()
