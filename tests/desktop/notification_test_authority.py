"""Explicit unlocked fixture for notification tests unrelated to native locking."""
class UnlockedAuthority:
    def __init__(self):self.facts={'known':True,'locked':False};self.changed=None
    def snapshot(self):return dict(self.facts)
    def refresh(self):return False
