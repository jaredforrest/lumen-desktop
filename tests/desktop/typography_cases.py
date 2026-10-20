"""Finite owned typography corpus and independently bounded presentation batches."""
CONSUMERS=('Files','Calendar')
SIZES={480:320,1060:720}
MODES=('light','dark')
SCALES=(1,1.15,1.3,1.5)
VIEWS={'Files':('list','grid','columns','gallery'),'Calendar':('Month','Year','Day','Week')}

def segments(consumer=None,width=None,mode=None):
 if consumer is not None and consumer not in CONSUMERS:raise ValueError('Unsupported consumer')
 if width is not None and width not in SIZES:raise ValueError('Unsupported window size')
 if mode is not None and mode not in MODES:raise ValueError('Unsupported colour mode')
 return [dict(consumer=c,width=w,mode=m)for c in CONSUMERS if consumer is None or c==consumer for w in SIZES if width is None or w==width for m in MODES if mode is None or m==mode]

def cases(consumer,width,mode):
 if segments(consumer,width,mode)!=[dict(consumer=consumer,width=width,mode=mode)]:raise ValueError('A complete segment is required')
 return [dict(app=consumer,width=width,height=SIZES[width],mode=mode,scale=scale,view=view)for scale in SCALES for view in VIEWS[consumer]]

def identity(case):
 return tuple(case[key]for key in ('app','width','height','mode','scale','view'))
