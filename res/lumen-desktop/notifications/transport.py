"""One pinned service instance and strictly newer UI snapshots per connection."""
import re
class TransportOrder:
    def __init__(self):self.instance=None;self.revision=0
    def accept(self,value):
        if not isinstance(value,dict) or not isinstance(value.get('instance'),str) or not re.fullmatch('[0-9a-f]{32}',value['instance']) or type(value.get('revision')) is not int or not 0<value['revision']<=2**53-1:return False
        if self.instance is not None and (value['instance']!=self.instance or value['revision']<=self.revision):return False
        self.instance,self.revision=value['instance'],value['revision'];return True
