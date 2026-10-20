"""Map inherited diagnostic pipes without pre-exec hooks in a threaded parent."""
import fcntl,os,sys
read,write=map(int,sys.argv[1:3]);command=sys.argv[3:]
if not command:raise SystemExit('Missing explicit application command')
copies=[fcntl.fcntl(fd,fcntl.F_DUPFD_CLOEXEC,10) for fd in (read,write)]
for source,target in zip(copies,(3,4)):os.dup2(source,target,inheritable=True)
for fd in set((read,write,*copies))-{3,4}:os.close(fd)
os.execv(command[0],command)
