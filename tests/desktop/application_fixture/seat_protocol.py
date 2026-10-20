"""Client-side focus-event ordering for complete private protocol traces."""
import re
EVENT=re.compile(r'\b(wl_keyboard|wl_pointer)#(\d+)\.(enter|leave)\(\d+, wl_surface#(\d+)')

def inspect(text):
    focus={};counts={'keyboard':{'enter':0,'leave':0},'pointer':{'enter':0,'leave':0}};errors=[]
    for number,line in enumerate(text.splitlines(),1):
        event=EVENT.search(line)
        if not event:continue
        kind,resource,action,surface=event.groups();key=(kind,resource)
        counts[kind.removeprefix('wl_')][action]+=1
        if action=='enter':focus[key]=surface
        else:
            expected=focus.pop(key,None)
            if expected!=surface:errors.append(dict(line=number,kind=kind,resource=resource,surface=surface,expected=expected))
    return dict(counts=counts,errors=errors)

def require_ordered(text):
    result=inspect(text)
    assert all(result['counts'][kind]['enter'] and result['counts'][kind]['leave'] for kind in ('keyboard','pointer')),'Complete keyboard and pointer trace required'
    assert not result['errors'],result['errors']
    return result
