"""Private supported Speech Dispatcher transport with no audible output."""
from pathlib import Path
import re


def configure(root,dispatcher,true_command):
    root=Path(root).resolve();dispatcher=Path(dispatcher).resolve();true_command=Path(true_command).absolute()
    if not true_command.is_absolute() or true_command.name!='true' or not true_command.is_file():raise ValueError('A fixed true executable is required')
    config=root/'speech-config';config.mkdir();(config/'modules').mkdir();logs=root/'speech-logs';logs.mkdir()
    module_dir=root/'speech-modules';module_dir.mkdir();(module_dir/'sd_generic').symlink_to(dispatcher/'libexec/speech-dispatcher-modules/sd_generic')
    socket=root/'speech.sock';protocol=root/'speech-protocol.log'
    empty_bin=root/'speech-empty-bin';empty_bin.mkdir()
    # Fixed no-audio executable; generic module quotes the ignored data argument.
    (config/'modules/observer.conf').write_text(f'''Debug 1
GenericExecuteSynth "{true_command} '$DATA'"
AddVoice "en" "MALE1" "observer"
DefaultVoice "observer"
GenericDefaultCharset "UTF-8"
''')
    (config/'speechd.conf').write_text('CommunicationMethod "unix_socket"\nSocketPath "'+str(socket)+'"\nLogLevel 5\nLogDir "'+str(logs)+'"\nCustomLogFile "protocol" "'+str(protocol)+'"\nAddModule "observer" "sd_generic" "observer.conf"\nDefaultModule "observer"\nDefaultLanguage "en"\nDisableAutoSpawn\n')
    command=[str(dispatcher/'bin/speech-dispatcher'),'--run-single','--config-dir='+str(config),'--module-dir='+str(module_dir),'--socket-path='+str(socket),'--pid-file='+str(root/'speech.pid'),'--timeout=0']
    return dict(command=command,address='unix_socket:'+str(socket),socket=socket,protocol=protocol,logs=logs,emptyBin=empty_bin)


def utterances(debug):
    """Orca's real speech-generator log; transport acceptance is checked separately."""
    return [match.group(1) for match in re.finditer(r'SPEECH OUTPUT: (.*)',debug)]


def source_proof(archive):
    import hashlib,tarfile
    with tarfile.open(archive) as source:
        member=next(value for value in source.getmembers() if value.name.endswith('/src/modules/generic.c'))
        if member.size>1024*1024:raise ValueError('Unexpected generic module source size')
        body=source.extractfile(member).read()
    digest=hashlib.sha256(body).hexdigest()
    if digest!='db6ee64b64ff1a057f452a3567af7d4ad4b4538bdacb7fcb86e9e6f7e36b1c7c':raise ValueError('Review changed speech module quoting before using this observer')
    if b'''message = g_string_append(message, "'\\\\''");''' not in body:raise ValueError('Missing single quote escaping')
    return dict(archive=str(archive),genericSourceSha256=digest,quotedDataArgument=True)


def transport_messages(protocol):
    """Reconstruct accepted SSIP SPEAK frames and their actual lifecycle replies."""
    clients={};collecting={};pending={};messages=[];by_id={}
    for match in re.finditer(r'\s(\d+):(DATA|REPLY):\|(.*?)\|(?: \(\d+\))?\n',protocol,re.S):
        descriptor,kind,payload=match.groups();value=payload.replace('\r\n','\n').rstrip('\n')
        if kind=='DATA':
            if value.startswith('SET self CLIENT_NAME '):clients[descriptor]=value.split(' ',3)[3]
            elif descriptor in collecting:
                if value=='.':pending[descriptor]=dict(client=clients.get(descriptor,''),text='\n'.join(collecting.pop(descriptor)),offset=match.start(),events=[])
                else:collecting[descriptor].append(value[1:] if value.startswith('..') else value)
            elif value=='SPEAK':collecting[descriptor]=[]
        elif '225 OK MESSAGE QUEUED' in value and descriptor in pending:
            identifier=re.search(r'225-(\d+)',value)
            if identifier:
                message=pending.pop(descriptor);message['id']=int(identifier.group(1));messages.append(message);by_id[message['id']]=message
        else:
            event=re.search(r'70[123]-(\d+).*?70[123] (BEGIN|END|CANCELED)',value,re.S)
            if event and int(event.group(1)) in by_id:by_id[int(event.group(1))]['events'].append(event.group(2))
    return messages


def plain_text(message):
    from xml.etree import ElementTree
    try:return ''.join(ElementTree.fromstring(message).itertext()) if message.lstrip().startswith('<speak') else message
    except ElementTree.ParseError:return message
