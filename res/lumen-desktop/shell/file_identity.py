"""Lossless local file identities across the JSON/JavaScript boundary."""
import re

MAX_UINT64=(1<<64)-1
MAX_SAFE_INTEGER=(1<<53)-1
DECIMAL=re.compile(r'^(?:0|[1-9][0-9]{0,19})$')

def component(value):
    if isinstance(value,str) and DECIMAL.fullmatch(value):
        parsed=int(value)
        if parsed<=MAX_UINT64:return parsed
    elif type(value) is int and 0<=value<=MAX_SAFE_INTEGER:
        return value
    raise ValueError('Invalid or imprecise file identity')

def parse(value):
    if not isinstance(value,dict) or set(value)!={'device','inode'}:raise ValueError('Invalid file identity')
    return {key:component(value[key]) for key in ('device','inode')}

def wire(info):
    values=dict(device=info.st_dev,inode=info.st_ino)
    if any(type(value) is not int or not 0<=value<=MAX_UINT64 for value in values.values()):raise ValueError('Unsupported native file identity')
    return {key:str(value) for key,value in values.items()}
