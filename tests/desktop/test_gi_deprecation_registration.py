"""Importing platform aliases is quiet; deliberate legacy access still diagnoses."""
import warnings
import gi
from gi import PyGIDeprecationWarning

gi.require_version('GLib','2.0')
gi.require_version('GioUnix','2.0')
with warnings.catch_warnings():
    warnings.simplefilter('error')
    from gi.repository import GLib,GioUnix
    assert GLib.MainLoop and GioUnix.DesktopAppInfo

# Verify deduplication itself, including the most recent replacement, rather than
# relying only on the current platform's duplicate alias being present.
from gi.overrides import deprecated_attr,_deprecated_attrs
try:
    deprecated_attr('LumenFixture','alias','OldTarget')
    deprecated_attr('LumenFixture','other','OtherTarget')
    deprecated_attr('LumenFixture','alias','LatestTarget')
    assert _deprecated_attrs['LumenFixture']==[('alias','LatestTarget'),('other','OtherTarget')]
finally:
    _deprecated_attrs.pop('LumenFixture',None)

with warnings.catch_warnings(record=True) as diagnostics:
    warnings.simplefilter('always')
    legacy=GLib.unix_signal_add_full
assert len(diagnostics)==1,diagnostics
assert issubclass(diagnostics[0].category,PyGIDeprecationWarning)
assert 'GLibUnix.signal_add' in str(diagnostics[0].message),diagnostics[0].message
assert callable(legacy)
print('PASS quiet GLib/GioUnix import, deduplicated registration with latest replacement, and one intentional legacy diagnostic')
