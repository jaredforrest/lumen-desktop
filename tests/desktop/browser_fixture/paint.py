"""Compare a real application readback with its presented native content crop."""
from collections import Counter

def match(reference,screen,origin):
 reference=reference.convert('RGB');screen=screen.convert('RGB');pixels=list(reference.get_flattened_data());background=Counter(pixels).most_common(1)[0][0]
 signature=[index for index,pixel in enumerate(pixels) if max(abs(pixel[channel]-background[channel]) for channel in range(3))>12]
 result=dict(painted=False,signaturePixels=len(signature),maximumRasterOffset=1,colorTolerance=3)
 if len(signature)<16:return result
 x,y=map(round,origin);best=None
 offsets=[(0,0)]+[(dx,dy) for dy in (-1,0,1) for dx in (-1,0,1) if dx or dy]
 for dx,dy in offsets:
  left,top=x+dx,y+dy
  if left<0 or top<0 or left+reference.width>screen.width or top+reference.height>screen.height:continue
  actual=list(screen.crop((left,top,left+reference.width,top+reference.height)).get_flattened_data())
  differences=[max(abs(a[channel]-b[channel]) for channel in range(3)) for a,b in zip(pixels,actual)]
  mismatch=sum(value>3 for value in differences)/len(differences);signature_mismatch=sum(differences[index]>3 for index in signature)/len(signature)
  value=dict(result,mismatchFraction=mismatch,signatureMismatchFraction=signature_mismatch,rasterOffset=[dx,dy],origin=[left,top],size=list(reference.size))
  if best is None or (signature_mismatch,mismatch)<(best['signatureMismatchFraction'],best['mismatchFraction']):best=value
  if mismatch<=.002 and signature_mismatch<=.01:return dict(value,painted=True)
 return best or result
