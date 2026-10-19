"""Fail review if excluding the unused scanner fixture could omit a binding test."""
from pathlib import Path
import re
import sys

source=Path(sys.argv[1])
project=(source/'subprojects/gobject-introspection-tests/meson.build').read_text()
for target,constructor in [('libwarnlib','library'),('warnlib_gir','gnome.generate_gir')]:
    block=re.search(r'^'+re.escape(target)+r' = '+re.escape(constructor)+r'\((.*?)^\)',project,re.M|re.S)
    assert block and block[1].count('build_by_default: false')==1, 'Review standalone warning-fixture target changes'

bindings=(source/'tests/meson.build').read_text()
dependencies=re.search(r'^test_deps = \[(.*?)\]',bindings,re.M|re.S)
assert dependencies, 'Review binding-test dependency declaration changes'
for name in ('gimarshallingtests_typelib','regress_typelib','utility_typelib'):
    assert name in dependencies[1], 'Required binding-test typelib removed'
for path in (source/'tests').rglob('*'):
    if path.suffix in ('.py','.c','.h','.build') or path.name=='meson.build':
        assert 'warnlib' not in path.read_text().lower(), 'WarnLib is now used by binding tests; review default target exclusion'
assert "option('tests', type : 'boolean', value : true" in (source/'meson_options.txt').read_text(), 'Upstream tests must remain enabled by default'
print('Verified standalone scanner fixture remains explicitly buildable; binding test dependencies unchanged')
