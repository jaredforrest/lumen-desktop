"""Retain genuine deprecations; expose canonical Action methods without old shadows."""
import hashlib,json,re,sys
from pathlib import Path
from xml.etree import ElementTree as ET
source,target,proof=map(Path,sys.argv[1:]);original=source.read_text();text=original
ns={'gi':'http://www.gtk.org/introspection/core/1.0'}
def action(value):return ET.fromstring(value).find('gi:namespace/gi:interface[@name="Action"]',ns)
root=action(text);changes=[]
for canonical,legacy in [('get_action_name','get_name'),('get_action_description','get_description')]:
 modern=root.find('gi:method[@name="'+canonical+'"]',ns);old=root.find('gi:method[@name="'+legacy+'"]',ns)
 assert modern.get('shadowed-by')==legacy and modern.get('deprecated') is None
 assert old.get('shadows')==canonical and old.get('deprecated')=='1' and old.get('deprecated-version')=='2.10'
 for name,attribute,value in [(canonical,'shadowed-by',legacy),(legacy,'shadows',canonical)]:
  pattern=r'(<method name="'+name+r'"\s+c:identifier="[^"]+")\n\s+'+attribute+r'="'+value+r'"'
  text,count=re.subn(pattern,r'\1',text);assert count==1,(name,count);changes.append(dict(method=name,removed=attribute,value=value))
fixed=action(text)
for name in ('get_name','get_description'):assert fixed.find('gi:method[@name="'+name+'"]',ns).get('deprecated')=='1'
for name in ('get_action_name','get_action_description'):assert fixed.find('gi:method[@name="'+name+'"]',ns).get('deprecated') is None
target.write_text(text);proof.write_text(json.dumps(dict(sourceSha256=hashlib.sha256(original.encode()).hexdigest(),correctedSha256=hashlib.sha256(text.encode()).hexdigest(),changes=changes,deprecatedAliasesRetained=True),indent=2))
