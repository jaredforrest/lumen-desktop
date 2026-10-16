"""Read-only classification audit. Outputs names/counts, never profile values."""
import argparse
import configparser
import json
from pathlib import Path


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--manifest',default='/etc/nix/configuration-boundaries.json')
    args=parser.parse_args()
    manifest=json.loads(Path(args.manifest).read_text())
    assert set(manifest['categories'])=={'config','state','cache'}
    assert all(rule['category'] in manifest['categories'] for rule in manifest['rules'])
    report={'categories':manifest['categories'],'rules':len(manifest['rules']),'craft':{},'prism':{}}
    for app,files in manifest['craft'].items():
        report['craft'][app]={}
        for name,fields in files.items():
            assert not set(fields['config'])&set(fields['state'])
            path=Path.home()/'.local/state/lumen-desktop/apps'/app/name
            value=json.loads(path.read_text()) if path.exists() else {}
            def leaves(item,prefix=''):
                for key,value in item.items():
                    path=prefix+key
                    if isinstance(value,dict):yield from leaves(value,path+'.')
                    else:yield path
            unexpected=sorted(key for key in leaves(value) if not any(key==field or key.startswith(field+'.') for field in fields['state']))
            report['craft'][app][name]={'retainedStatePresent':path.exists(),'unclassifiedStateFields':unexpected}
    prism=manifest['prism'];path=Path(prism['path'])
    cfg=configparser.ConfigParser(interpolation=None);cfg.optionxform=str
    if path.exists():cfg.read(path)
    fields=set(cfg['General']) if cfg.has_section('General') else set()
    groups={category:set(prism[category]) for category in manifest['categories']}
    assert not groups['config']&groups['state'] and not groups['config']&groups['cache'] and not groups['state']&groups['cache']
    report['prism']={'present':path.exists(),'counts':{key:len(fields&value) for key,value in groups.items()},
                     'unclassifiedFieldNames':sorted(fields-set().union(*groups.values())),
                     'adapterInstalled':Path('/etc/nix/desktop-settings.json').exists()}
    print(json.dumps(report,indent=2))
    return int(bool(report['prism']['unclassifiedFieldNames']) or any(row['unclassifiedStateFields'] for app in report['craft'].values() for row in app.values()))

if __name__=='__main__':raise SystemExit(main())
