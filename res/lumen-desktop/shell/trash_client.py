"""Typed Trash client; accepted work belongs to the existing user file service."""
import json,sys
from file_jobs import remote
MAX_REQUEST=1024**2-1

def main():
    while True:
        line=sys.stdin.buffer.readline(MAX_REQUEST+1)
        if not line:break
        if len(line)>MAX_REQUEST:
            print(json.dumps(dict(id=None,error='Trash request exceeds its declared limit')),flush=True);break
        request={}
        try:
            if len(line)>MAX_REQUEST:raise ValueError('Trash request exceeds its declared limit')
            request=json.loads(line)
            if not isinstance(request,dict) or type(request.get('id')) is not int or not 1<=request['id']<=9007199254740991:raise ValueError('Invalid Trash request identity')
            if not isinstance(request.get('action'),str) or not request['action'].startswith('trash-'):raise ValueError('Unsupported Trash client request')
            payload={key:value for key,value in request.items() if key!='id'}
            result=remote(payload);result.update(id=request['id'],action=request['action'])
        except Exception as error:result=dict(id=request.get('id') if isinstance(request,dict) else None,error=str(error)[:1024])
        print(json.dumps(result),flush=True)
if __name__=='__main__':main()
