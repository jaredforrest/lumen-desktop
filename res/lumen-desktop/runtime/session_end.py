"""Cancellable cooperative preflight; never invokes a host action or kills an app."""
import os,uuid
from pathlib import Path

class SessionEnd:
    def __init__(self,lifecycle,cooperative,shell_pid=None):
        self.lifecycle=lifecycle;self.cooperative=cooperative;self.shell_pid=shell_pid;self.shell_identity=self.shell_process();self.cancel()
    def shell_process(self):
        if self.shell_pid is None:return None
        try:
            root=Path("/proc")/str(self.shell_pid);executable=(root/"exe").resolve();info=executable.stat()
            if root.stat().st_uid!=os.getuid() or "quickshell" not in executable.name:return None
            start=int((root/"stat").read_text().rsplit(")",1)[1].split()[19])
            return (start,info.st_dev,info.st_ino)
        except (OSError,ValueError,IndexError):return None
    def cancel(self):
        self.token='';self.phase='idle';self.action='';self.error='';self.entries=[];self.windows={};self.current=None;self.epoch=None;self.identities={};self.checking=[]
    def state(self):
        return dict(token=self.token,phase=self.phase,action=self.action,error=self.error,waiting=self.current[0] if self.current else '',remaining=len(self.entries)+(1 if self.current else 0))
    def fail(self,message):
        self.phase='blocked';self.error=message;self.entries=[];self.current=None
    def external(self,nodes):
        shell_valid=self.shell_identity is not None and self.shell_process()==self.shell_identity
        return [n for n in nodes if not (shell_valid and n.get('pid')==self.shell_pid and n.get('app_id')=='org.quickshell')]
    def begin(self,action,nodes):
        self.cancel()
        if action not in ('logout','reboot','poweroff'):raise ValueError('Unsupported session action')
        self.action=action;self.token=uuid.uuid4().hex;self.epoch=self.lifecycle.epoch;self.phase='checking'
        external=self.external(nodes);self.windows={n['id']:(n.get('pid'),n.get('app_id')) for n in external}
        try:
            if any(n.get('shell')!='xdg_shell' or not n.get('app_id') for n in external):raise ValueError('An open application has no supported native cooperative protocol. Quit it using its own controls, then try again.')
            apps=self.lifecycle.rows()
            if any(len(process['apps'])!=1 for process in self.lifecycle.processes.values()):raise ValueError('Shared process ownership is ambiguous')
            if any(n.get('pid') not in self.lifecycle.processes or n.get('app_id') not in self.lifecycle.processes[n['pid']]['apps'] for n in external):raise ValueError('An open application cannot be identified safely. Quit it using its own controls, then try again.')
            # Validate every endpoint before asking any application to quit.
            self.identities={pid:process['identity'].copy() for pid,process in self.lifecycle.processes.items()}
            self.checking=[row['appId'] for row in apps]
            self.tick(nodes)
        except Exception:
            self.fail('Cooperative Quit is unavailable or ownership is ambiguous for an open application. No further applications will be asked to quit. Use application-owned Quit controls, then try again.')
        return self.state()
    def validate(self,nodes):
        if self.lifecycle.epoch!=self.epoch:raise ValueError('The compositor session changed. Start again.')
        for process in self.lifecycle.processes.values():
            if self.identities.get(process['identity']['pid'])!=process['identity']:raise ValueError('An application started or changed. Start again.')
        for node in self.external(nodes):
            descriptor=(node.get('pid'),node.get('app_id'))
            if self.windows.get(node['id'])==descriptor:continue
            relationship=node.get('lumen_relationships') or {}
            parent=relationship.get('parent')
            if parent in self.windows and self.windows[parent]==descriptor:continue
            process=self.lifecycle.processes.get(node.get('pid'))
            if relationship.get('modal') is True and process and process['apps']=={node.get('app_id')}:continue
            raise ValueError('A new application window appeared. Start again after resolving it.')
    def tick(self,nodes):
        if self.phase not in ('checking','waiting','ready'):return
        try:
            self.validate(nodes)
            if self.phase=='checking':
                if self.checking:
                    app=self.checking.pop(0)
                    try:self.entries.append(self.cooperative.prepare_quit(app))
                    except Exception:
                        self.fail('Cooperative Quit is unavailable for '+app+'. Quit it using its own controls, then try again.');return
                    if self.checking:return
                self.phase='waiting'
            if self.current:
                process=self.current[1]
                if process not in self.lifecycle.processes.values():self.current=None
                else:return
            while self.entries:
                entry=self.entries.pop(0);process=entry[1]
                # A previously captured process may already have exited voluntarily.
                if process not in self.lifecycle.processes.values():continue
                self.cooperative.dispatch_quit(entry);self.current=entry;return
            self.phase='ready'
        except Exception:self.fail('Applications or their windows changed, or a cooperative Quit failed. The session action was stopped. Resolve applications and start again.')
    def authorize(self,token,nodes):
        if not token or token!=self.token or self.phase!='ready':raise ValueError('Session confirmation is no longer ready')
        self.tick(nodes)
        if self.phase!='ready' or self.lifecycle.processes:raise ValueError('Applications remain open; the session action was stopped')
        action=self.action;self.cancel()
        return dict(action=action,token=token)
