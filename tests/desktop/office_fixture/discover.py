"""Read installed headless command metadata inside the application boundary."""
import json,subprocess,sys
from pathlib import Path
commands={
 'wordcraft':[['--version'],['commands','--json']],
 'gridcraft':[['version'],['commands','--json'],['functions','--json']],
 'deckcraft':[['--version'],['commands']],
}
packages=json.loads(Path(sys.argv[1]).read_text())
results={}
for name,arguments in commands.items():
 results[name]=[]
 for index,args in enumerate(arguments):
  result=subprocess.run([packages[name]+'/bin/'+name+'-cli',*args],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,timeout=30)
  stem=Path('/artifacts')/(name+'-'+str(index));stem.with_suffix('.stdout').write_text(result.stdout);stem.with_suffix('.stderr').write_text(result.stderr)
  results[name].append(dict(arguments=args,returncode=result.returncode,stdout=str(stem.with_suffix('.stdout')),stderr=str(stem.with_suffix('.stderr'))))
Path('/artifacts/discovery.json').write_text(json.dumps(results,indent=2))
