"""Geometry oracle for declared frame boundaries in native pixel fixtures."""
import math


def outside_frame(x, y, width, height, radius, tolerance):
    """Test a logical pixel centre against the active frame and raster tolerance.

    A zero radius is intentionally rectangular. The tolerance is supplied in
    logical units by the capture's physical-pixel mapping, not a fixed inset.
    """
    if not all(math.isfinite(value) for value in (x,y,width,height,radius,tolerance)):
        raise ValueError('Frame coordinates must be finite')
    if width<=0 or height<=0 or radius<0 or tolerance<0:
        raise ValueError('Invalid declared frame dimensions')
    if x < -tolerance or y < -tolerance or x > width+tolerance or y > height+tolerance:
        return True
    radius=min(radius,width/2,height/2)
    if radius==0:
        return False
    cx=min(max(x,radius),width-radius)
    cy=min(max(y,radius),height-radius)
    return (x-cx)**2+(y-cy)**2 > (radius+tolerance)**2


def raster_origin(value, scale):
    """Match scene scale_box: round absolute output-local positions, ties away."""
    if not math.isfinite(value) or not math.isfinite(scale) or scale<=0:
        raise ValueError('Invalid output mapping')
    physical=value*scale
    return math.floor(physical+.5) if physical>=0 else math.ceil(physical-.5)
