"""Apply a narrow visual patch to the pinned LocalSend source; never patch its protocol."""
import argparse
from pathlib import Path
import shutil

def replace_once(text,old,new):
    if text.count(old)!=1:raise ValueError('Pinned LocalSend presentation source changed: '+old[:55])
    return text.replace(old,new,1)

def apply(root,font):
    root=Path(root);assets=Path(__file__).parent
    theme=root/'lib/config/theme.dart';text=theme.read_text()
    text=replace_once(text,'  return ThemeData(','  final desktopBase = ThemeData(')
    text=replace_once(text,'    fontFamily: fontFamily,\n  );','    fontFamily: fontFamily,\n  );\n  return checkPlatform([TargetPlatform.linux]) ? desktopAppearance(desktopBase, brightness) : desktopBase;')
    theme.write_text(text+'\n'+(assets/'localsend-theme.dart').read_text())
    home=root/'lib/pages/home_page.dart';text=home.read_text();start=text.index('                  NavigationRail(');end=text.index('                Expanded(',start)
    text=text[:start]+(assets/'localsend_sidebar.dart').read_text()+text[end:];home.write_text(text)
    spec=root/'pubspec.yaml';text=spec.read_text();text=replace_once(text,'flutter:\n  uses-material-design: true','flutter:\n  uses-material-design: true\n  fonts:\n    - family: Interlude Variable\n      fonts:\n        - asset: assets/fonts/InterludeVariable.ttf')
    spec.write_text(text);target=root/'assets/fonts/InterludeVariable.ttf';target.parent.mkdir(exist_ok=True,parents=True);shutil.copyfile(font,target)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('root',type=Path);p.add_argument('font',type=Path);a=p.parse_args();apply(a.root,a.font)
