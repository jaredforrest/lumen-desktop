"""Fit an owned floating client within an outer budget using native geometry."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import socket
import time

from application_lifecycle import receive, send
import sys


def walk(node,workspace=None,floating=False):
    if node.get('type')=='workspace':workspace=node
    floating=floating or node.get('type')=='floating_con'
    yield node,workspace,floating
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):
        yield from walk(child,workspace,floating)


def matching_windows(tree,app,title,pid,container=0):
    return [row for row in walk(tree) if row[0].get('app_id')==app and row[0].get('name')==title and row[0].get('pid')==pid and (not container or row[0].get('id')==container)]


def exact_window(tree,app,title,pid,container=0):
    rows=matching_windows(tree,app,title,pid,container)
    if len(rows)!=1:raise ValueError('Owned window is unavailable or ambiguous')
    return rows[0]


def integer(value,name,minimum=0):
    if type(value) is not int or value<minimum:raise ValueError('Invalid '+name)
    return value


def rectangle(value,name):
    if not isinstance(value,dict):raise ValueError('Missing '+name)
    return {key:integer(value.get(key),name+' '+key,1 if key in ('width','height') else -2147483648) for key in ('x','y','width','height')}


def criterion(value):
    if not isinstance(value,str) or not value:raise ValueError('Invalid window identity')
    # Sway only unescapes quoted double quotes, not JSON backslash escapes.
    # PCRE2 UTF code-point literals avoid command syntax and regex injection,
    # including quotes, backslashes, non-ASCII names and embedded newlines.
    return '"'+r'\A'+''.join(r'\x{%x}'%ord(char) for char in value)+r'\z'+'"'


def fit_plan(row,budget_width,budget_height,min_width,min_height,margin_width=None,margin_height=None):
    node,workspace,floating=row
    for value,name in [(budget_width,'outer width'),(budget_height,'outer height'),(min_width,'minimum width'),(min_height,'minimum height')]:integer(value,name,1)
    state=node.get('lumen_state') or {};pair=node.get('lumen_pair') or {};relation=node.get('lumen_relationships') or {}
    if not floating or node.get('type')!='floating_con' or node.get('fullscreen_mode') or node.get('scratchpad_state') not in (None,'none') or pair.get('id') or pair.get('survivor') or any(state.get(key) for key in ('filled','minimised','hidden','desktop_hidden')) or relation.get('parent') is not None or relation.get('modal') is True:
        return dict(changed=False,supported=False,reason='Only visible independent floating windows are fitted'),None
    if workspace is None or workspace.get('name')=='__i3_scratch':raise ValueError('Window workspace unavailable')
    box=rectangle(node.get('rect'),'container rectangle');client=rectangle(node.get('window_rect'),'client rectangle');area=rectangle(workspace.get('rect'),'workspace rectangle')
    deco=node.get('deco_rect') or {};deco_height=integer(deco.get('height',0),'decoration height')
    pad_width=box['width']-client['width'];pad_height=box['height']+deco_height-client['height']
    if pad_width<0 or pad_height<0:raise ValueError('Inconsistent native window padding')
    if margin_width is not None:
        budget_width=max(1,area['width']-integer(margin_width,'width margin'))
        budget_height=max(1,area['height']-integer(margin_height,'height margin'))
    available_width=min(budget_width,area['width']);available_height=min(budget_height,area['height'])
    if min_width+pad_width>area['width'] or min_height+pad_height>area['height']:
        return dict(changed=False,supported=False,reason='Minimum client size exceeds usable workspace'),None
    width=min(client['width'],max(min_width,available_width-pad_width))
    height=min(client['height'],max(min_height,available_height-pad_height))
    outer_width=width+pad_width;outer_height=height+pad_height
    x=box['x'];y=box['y']-deco_height
    outside=x<area['x'] or y<area['y'] or x+outer_width>area['x']+area['width'] or y+outer_height>area['y']+area['height']
    resized=width!=client['width'] or height!=client['height']
    result=dict(changed=resized or outside,supported=True,clientWidth=width,clientHeight=height,paddingWidth=pad_width,paddingHeight=pad_height,outerWidth=outer_width,outerHeight=outer_height,budgetWidth=available_width,budgetHeight=available_height,minimumLimitsMargin=outer_width>available_width or outer_height>available_height,recentred=outside)
    if not result['changed']:return result,None
    identity='con_id=%d pid=%d app_id=%s title=%s' % (integer(node.get('id'),'container id',1),integer(node.get('pid'),'owner PID',1),criterion(node.get('app_id')),criterion(node.get('name')))
    operations=[]
    if resized:operations.append('resize set width %d px height %d px'%(outer_width,outer_height))
    if outside:operations.append('move position center')
    return result,'['+identity+'] '+', '.join(operations)


def process_start(pid):
    fields=Path('/proc/%d/stat'%pid).read_text().rsplit(') ',1)[1].split()
    return integer(int(fields[19]),'process start',1)


def socket_identity(path):
    stat=Path(path).stat();return stat.st_dev,stat.st_ino,stat.st_mtime_ns


def read_tree():
    return json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],timeout=4))


def mapped_window(app,title,pid,container):
    deadline=time.monotonic()+4
    rows=matching_windows(read_tree(),app,title,pid,container)
    if len(rows)==1:return rows[0]
    if len(rows)>1 or container:raise ValueError('Owned window is unavailable or ambiguous')
    # Subscribe before re-sampling to close the initial-map race. This is an
    # acknowledged event wait, not polling or choosing a later replacement ID.
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as events:
        events.settimeout(max(.001,deadline-time.monotonic()));events.connect(os.environ['SWAYSOCK'])
        send(events,2,'["window"]');kind,reply=receive(events)
        if kind!=2 or not isinstance(reply,dict) or reply.get('success') is not True:raise ValueError('Native map subscription rejected')
        while True:
            rows=matching_windows(read_tree(),app,title,pid,0)
            if len(rows)==1:return rows[0]
            if len(rows)>1:raise ValueError('Owned window is ambiguous')
            remaining=deadline-time.monotonic()
            if remaining<=0:raise ValueError('Owned window did not map within the request budget')
            events.settimeout(remaining)
            kind,event=receive(events)
            if kind!=0x80000003 or not isinstance(event,dict):raise ValueError('Invalid native window event')


def preferred_plan(row, width, height):
    integer(width, 'preferred width', 1)
    integer(height, 'preferred height', 1)
    if width > 32768 or height > 32768:raise ValueError('Preferred size exceeds supported bounds')
    node = row[0]
    identity = 'con_id=%d pid=%d app_id=%s title=%s' % (integer(node.get('id'),'container id',1),integer(node.get('pid'),'owner PID',1),criterion(node.get('app_id')),criterion(node.get('name')))
    return '[' + identity + '] lumen_window preferred-size %d %d' % (width,height)


def execute(app,title,container,budget_width,budget_height,min_width,min_height,margin_width=None,margin_height=None,preferred_width=None,preferred_height=None):
    owner=os.getppid();start=process_start(owner);session=socket_identity(os.environ['SWAYSOCK'])
    preferred_supported = False
    if preferred_width is not None:
        version = json.loads(subprocess.check_output(['swaymsg','-r','-t','get_version'],timeout=4))
        preferred_supported = isinstance(version,dict) and version.get('native_preferred_zoom') is True
    first=mapped_window(app,title,owner,container)
    node=first[0];captured=(node['id'],node['pid'],node['app_id'],node['name'])
    # A replacement with the same title is never resolved as a new target.
    second=exact_window(read_tree(),app,title,owner,captured[0])
    actual=second[0]
    if captured!=(actual['id'],actual['pid'],actual['app_id'],actual['name']) or process_start(owner)!=start or socket_identity(os.environ['SWAYSOCK'])!=session:
        raise ValueError('Owned window identity changed before fitting')
    result,command=fit_plan(second,budget_width,budget_height,min_width,min_height,margin_width,margin_height)
    result.update(container=captured[0],pid=owner,processStart=start,app=app,title=title)
    if preferred_width is not None:
        registration = preferred_plan(second, preferred_width, preferred_height)
        result['preferredSizeSupported'] = preferred_supported
        if result['preferredSizeSupported']:
            command = registration + ('; ' + command if command else '')
            result['preferredSize'] = dict(width=preferred_width,height=preferred_height)
    if command:
        replies=json.loads(subprocess.check_output(['swaymsg','-r',command],timeout=4))
        if not replies or not all(reply.get('success') for reply in replies):raise ValueError('Native fit request rejected')
    return result


def watch(app,title,container):
    owner=os.getppid();start=process_start(owner);session=socket_identity(os.environ['SWAYSOCK'])
    captured=container or None;previous=None
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as events:
        events.connect(os.environ['SWAYSOCK']);send(events,2,'["window","output"]');kind,reply=receive(events)
        if kind!=2 or not isinstance(reply,dict) or reply.get('success') is not True:raise ValueError('Native geometry subscription rejected')
        print(json.dumps(dict(ready=True)),flush=True)
        while True:
            if process_start(owner)!=start or socket_identity(os.environ['SWAYSOCK'])!=session:raise ValueError('Owned geometry session changed')
            rows=matching_windows(read_tree(),app,title,owner,captured or 0)
            if len(rows)==1:
                node,workspace,_=rows[0]
                if captured is None:captured=node['id']
                if workspace:
                    area=rectangle(workspace.get('rect'),'workspace rectangle')
                    fingerprint=(workspace['id'],tuple(area.items()))
                    if fingerprint!=previous:
                        previous=fingerprint
                        print(json.dumps(dict(changed=True,container=captured,pid=owner,app=app,title=title,workspace=workspace['id'],area=area)),flush=True)
            kind,event=receive(events)
            if kind not in (0x80000003,0x80000001) or not isinstance(event,dict):raise ValueError('Invalid native geometry event')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app',required=True);parser.add_argument('--title',required=True)
    parser.add_argument('--container',type=int,default=0)
    parser.add_argument('--watch',action='store_true')
    parser.add_argument('--margin-width',type=int);parser.add_argument('--margin-height',type=int)
    parser.add_argument('--outer-width',type=int,default=1);parser.add_argument('--outer-height',type=int,default=1)
    parser.add_argument('--minimum-width',type=int,default=1);parser.add_argument('--minimum-height',type=int,default=1)
    parser.add_argument('--preferred-width',type=int);parser.add_argument('--preferred-height',type=int)
    args=parser.parse_args()
    try:
        if args.container<0:raise ValueError('Invalid captured container')
        if (args.margin_width is None)!=(args.margin_height is None):raise ValueError('Both native workspace margins are required')
        if (args.preferred_width is None)!=(args.preferred_height is None):raise ValueError('Both preferred dimensions are required')
        if args.watch:watch(args.app,args.title,args.container);return 0
        result=execute(args.app,args.title,args.container,args.outer_width,args.outer_height,args.minimum_width,args.minimum_height,args.margin_width,args.margin_height,args.preferred_width,args.preferred_height)
        print(json.dumps(result));return 0
    except (OSError,EOFError,KeyError,IndexError,ValueError,subprocess.SubprocessError) as error:
        print(json.dumps(dict(error=str(error))));return 1


if __name__=='__main__':sys.exit(main())
