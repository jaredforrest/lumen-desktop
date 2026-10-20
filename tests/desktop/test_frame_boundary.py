"""The clipping oracle preserves square clients and only admits raster tolerance."""
import unittest
from frame_boundary import outside_frame, raster_origin


class FrameBoundary(unittest.TestCase):
    def test_square_client_keeps_its_corners(self):
        for x,y in [(0,0),(200,0),(0,100),(200,100),(100,50)]:
            self.assertFalse(outside_frame(x,y,200,100,0,0))
        self.assertTrue(outside_frame(-.01,50,200,100,0,0))

    def test_all_four_rounded_corners_reject_square_child_pixels(self):
        for x,y in [(0,0),(200,0),(0,100),(200,100)]:
            self.assertTrue(outside_frame(x,y,200,100,12,0))
        for x,y in [(12,0),(188,0),(0,12),(200,12),(100,50)]:
            self.assertFalse(outside_frame(x,y,200,100,12,0))

    def test_one_physical_pixel_is_scaled_not_a_logical_pixel(self):
        for scale in [1,1.25,1.5,2]:
            self.assertFalse(outside_frame(-.9/scale,50,200,100,0,1/scale))
            self.assertTrue(outside_frame(-1.1/scale,50,200,100,0,1/scale))
            self.assertFalse(outside_frame(12,-.9/scale,200,100,12,1/scale))
            self.assertTrue(outside_frame(12,-1.1/scale,200,100,12,1/scale))

    def test_absolute_output_origin_uses_native_rounding(self):
        self.assertEqual(raster_origin(245,1.5),368)
        self.assertEqual(raster_origin(243,1.5),365)
        self.assertEqual(raster_origin(-243,1.5),-365)
        for scale in [1,1.25,1.5,2]:
            for value in [0,1,243,245,-243]:
                self.assertLessEqual(abs(raster_origin(value,scale)-value*scale),.5)
        with self.assertRaises(ValueError):raster_origin(1,0)

    def test_invalid_geometry_cannot_silently_disable_the_oracle(self):
        for values in [(0,0,0,100,12,1),(0,0,200,100,-1,1),(0,0,200,100,12,-1),(float('nan'),0,200,100,12,1)]:
            with self.assertRaises(ValueError):outside_frame(*values)


if __name__=='__main__':unittest.main()
